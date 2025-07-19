use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use colored::Colorize;
use notify::{RecursiveMode, Watcher};
use std::sync::mpsc::channel;

use crate::tui::{format_process_name, Process, ProcessState};
use crate::{prelude::*, tui};

mod compilation_result;

/// Run compilation without process tree visualization
pub fn run(source: miette::NamedSource<String>) -> i64 {
    // Helper function to handle panics and convert them to errors
    fn handle_panic(
        panic: Box<dyn std::any::Any + Send>,
        stage: &str,
        source: &miette::NamedSource<String>,
    ) {
        let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
            msg.clone()
        } else if let Some(msg) = panic.downcast_ref::<&str>() {
            msg.to_string()
        } else {
            format!("Unknown {} error", stage)
        };

        let error = Error::Compiler(CompilerError::CodeGenerationFailed {
            span: Span::default(),
            help: format!("{} failed: {}", stage, panic_message),
        });

        eprintln!(
            "{:?}",
            miette::miette!(error).with_source_code(source.clone())
        );
        std::process::exit(1);
    }

    // Stage 1: Lexing - catch panics
    let lexer = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Lexer::new(source.inner().as_str())
    })) {
        Ok(lexer) => lexer,
        Err(panic) => {
            handle_panic(panic, "Lexing", &source);
            unreachable!()
        }
    };

    // Stage 2: Parsing - catch panics
    let parsed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut parser = Parser::new(lexer);
        parser.parse()
    })) {
        Ok(Ok(parsed)) => parsed,
        Ok(Err(errors)) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Parsing", &source);
            unreachable!()
        }
    };

    // Stage 3: Type checking - catch panics
    let type_checked = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut type_checker = TypeChecker::new();
        type_checker.check(&parsed)
    })) {
        Ok(Ok(typed_statement)) => typed_statement,
        Ok(Err(errors)) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Type checking", &source);
            unreachable!()
        }
    };

    // Stage 4: Code generation - catch panics
    let (compiled, return_type) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut compiler = Compiler::new();
        compiler.compile(&type_checked)
    })) {
        Ok(Ok(result)) => result, // Successful compilation
        Ok(Err(error)) => {
            // Regular error
            eprintln!(
                "{:?}",
                miette::miette!(error).with_source_code(source.clone())
            );
            std::process::exit(1);
        }
        Err(panic) => {
            // Panic occurred during compilation
            let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
                msg.clone()
            } else if let Some(msg) = panic.downcast_ref::<&str>() {
                msg.to_string()
            } else {
                "Unknown compilation error".to_string()
            };

            let error = Error::Compiler(CompilerError::CodeGenerationFailed {
                span: type_checked.span,
                help: format!("Code generation failed: {}", panic_message),
            });

            eprintln!(
                "{:?}",
                miette::miette!(error).with_source_code(source.clone())
            );
            std::process::exit(1);
        }
    };

    // Stage 5: Execution - catch panics
    let return_value = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let runner = Runner::new();
        runner.run(compiled, &return_type)
    })) {
        Ok(Ok(return_value)) => return_value,
        Ok(Err(error)) => {
            eprintln!("Runtime error: {:?}", error);
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Execution", &source);
            unreachable!()
        }
    };

    return_value
}

/// Run compilation with process tree visualization
pub fn run_with_process_tree(source: miette::NamedSource<String>) -> Option<i64> {
    use compilation_result::COMPILED_CODE;

    // Reset the compiled code storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.code = None;
        code_storage.return_type = None;
    }

    let now = SystemTime::now();

    // Create the process tree for compilation stages
    let process_tree = Arc::new(Mutex::new(Process {
        name: format!("{}", source.name()),
        state: ProcessState::Running,
        note: Some("lexing".to_string()),
        started_at: now,
        completed_at: None,
        children: vec![],
    }));

    // Clone for the compilation thread
    let tree_clone = Arc::clone(&process_tree);
    let source_clone = source.clone();

    // Run compilation in a separate thread
    let compilation_result = Arc::new(Mutex::new(None));
    let result_clone = Arc::clone(&compilation_result);

    let _compilation_thread = thread::spawn(move || {
        let result = run_compilation_stages(source_clone, tree_clone);
        *result_clone.lock().unwrap() = Some(result);
    });

    // Display the animated tree
    loop {
        {
            let tree = process_tree.lock().unwrap();
            crate::tui::draw_process_tree_animated(&tree);
        }

        // Check if compilation is done
        let compilation_guard = compilation_result.lock().unwrap();
        if let Some(result) = compilation_guard.as_ref() {
            match result {
                Ok(_) => {
                    drop(compilation_guard); // Release the lock

                    // Update main process to completed
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Completed;
                        tree.note = None;
                        tree.completed_at = Some(SystemTime::now());
                    }

                    // Final draw of the process tree
                    {
                        let tree = process_tree.lock().unwrap();
                        crate::tui::draw_process_tree_animated(&tree);
                    }

                    eprintln!("");

                    // Print that the compilation completed
                    tui::print_success(format!(
                        "compilation {}",
                        format_process_name("succeeded", &ProcessState::Completed).bright_green()
                    ));

                    eprintln!("");

                    // Retrieve the compiled code from global storage
                    let (code_ptr, return_type) = {
                        let code_storage = COMPILED_CODE.lock().unwrap();
                        match (&code_storage.code, &code_storage.return_type) {
                            (Some(ptr), Some(rt)) => (*ptr, rt.clone()),
                            _ => {
                                tui::print_error("No compiled code available!".to_string());
                                return None;
                            }
                        }
                    };

                    // Execute the compiled code - catch panics
                    let return_value =
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let runner = Runner::new();
                            runner.run(code_ptr, &return_type)
                        })) {
                            Ok(Ok(return_value)) => return_value,
                            Ok(Err(error)) => {
                                tui::print_error(format!("Runtime error: {:?}", error));
                                return None;
                            }
                            Err(panic) => {
                                let panic_message =
                                    if let Some(msg) = panic.downcast_ref::<String>() {
                                        msg.clone()
                                    } else if let Some(msg) = panic.downcast_ref::<&str>() {
                                        msg.to_string()
                                    } else {
                                        "Unknown runtime error".to_string()
                                    };
                                tui::print_error(format!(
                                    "Runtime execution failed: {}",
                                    panic_message
                                ));
                                return None;
                            }
                        };
                    return Some(return_value);
                }
                Err(error_reports) => {
                    // Update main process to error
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Error;
                        tree.note = format!(
                            "{} {}",
                            error_reports.len(),
                            if error_reports.len() == 1 {
                                "error"
                            } else {
                                "errors"
                            }
                        )
                        .into();
                        tree.completed_at = Some(SystemTime::now());
                    }

                    // Show error state for a moment
                    {
                        let tree = process_tree.lock().unwrap();
                        crate::tui::draw_process_tree_animated(&tree);
                    }

                    eprintln!("");

                    // Print miette errors with proper formatting
                    for error_report in error_reports {
                        eprintln!("{:?}", error_report);
                    }

                    // Print that the compilation failed with x errors
                    tui::print_error(format!(
                        "compilation {} with {} {}",
                        format_process_name("failed", &ProcessState::Error).bright_green(),
                        error_reports.len(),
                        if error_reports.len() == 1 {
                            "error"
                        } else {
                            "errors"
                        }
                    ));

                    eprintln!("");

                    return None;
                }
            }
        } else {
            drop(compilation_guard); // Release the lock
        }

        // 10fps for animations
        thread::sleep(Duration::from_millis(100));
    }
}

/// Run compilation stages with process tree updates
/// Instead of returning the raw pointer, we store it in a global and return success/failure
fn run_compilation_stages(
    source: miette::NamedSource<String>,
    process_tree: Arc<Mutex<Process>>,
) -> std::result::Result<(), Vec<miette::Report>> {
    use compilation_result::COMPILED_CODE;

    fn update_stage_note(tree: &Arc<Mutex<Process>>, stage_name: &str) {
        let mut tree = tree.lock().unwrap();
        tree.note = Some(stage_name.to_string());
    }

    // Helper function to handle panics and convert them to error reports
    fn handle_panic(
        panic: Box<dyn std::any::Any + Send>,
        stage: &str,
        source: &miette::NamedSource<String>,
    ) -> Vec<miette::Report> {
        let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
            msg.clone()
        } else if let Some(msg) = panic.downcast_ref::<&str>() {
            msg.to_string()
        } else {
            format!("Unknown {} error", stage)
        };

        let error = Error::Compiler(CompilerError::CodeGenerationFailed {
            span: Span::default(),
            help: format!("{} failed: {}", stage, panic_message),
        });

        let report = miette::miette!(error).with_source_code(source.clone());
        vec![report]
    }

    // Stage 1: Lexing - catch panics
    update_stage_note(&process_tree, "lexing");

    let lexer = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Lexer::new(source.inner().as_str())
    })) {
        Ok(lexer) => lexer,
        Err(panic) => {
            return Err(handle_panic(panic, "Lexing", &source));
        }
    };

    // Stage 2: Parsing - catch panics
    update_stage_note(&process_tree, "parsing");

    let parsed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut parser = Parser::new(lexer);
        parser.parse()
    })) {
        Ok(Ok(parsed)) => parsed,
        Ok(Err(errors)) => {
            let mut error_messages = Vec::new();
            for error in errors {
                let report = miette::miette!(error).with_source_code(source.clone());
                error_messages.push(report);
            }
            return Err(error_messages);
        }
        Err(panic) => {
            return Err(handle_panic(panic, "Parsing", &source));
        }
    };

    // Stage 3: Type checking - catch panics
    update_stage_note(&process_tree, "type checking");

    let type_checked = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut type_checker = TypeChecker::new();
        type_checker.check(&parsed)
    })) {
        Ok(Ok(typed_statement)) => typed_statement,
        Ok(Err(errors)) => {
            let mut error_messages = Vec::new();
            for error in errors {
                let report = miette::miette!(error).with_source_code(source.clone());
                error_messages.push(report);
            }
            return Err(error_messages);
        }
        Err(panic) => {
            return Err(handle_panic(panic, "Type checking", &source));
        }
    };

    // Stage 4: Code generation - catch panics
    update_stage_note(&process_tree, "code generation");

    let (compiled, return_type) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut compiler = Compiler::new();
        compiler.compile(&type_checked)
    })) {
        Ok(Ok(result)) => result, // Successful compilation
        Ok(Err(error)) => {
            // Regular error
            let report = miette::miette!(error).with_source_code(source.clone());
            return Err(vec![report]);
        }
        Err(panic) => {
            // Panic occurred during compilation
            let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
                msg.clone()
            } else if let Some(msg) = panic.downcast_ref::<&str>() {
                msg.to_string()
            } else {
                "Unknown compilation error".to_string()
            };

            let error = Error::Compiler(CompilerError::CodeGenerationFailed {
                span: type_checked.span,
                help: format!("Code generation failed: {}", panic_message),
            });

            let report = miette::miette!(error).with_source_code(source.clone());
            return Err(vec![report]);
        }
    };

    // Store the compiled code in our global storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.set_code(compiled, return_type);
    }

    // Return success
    Ok(())
}

/// Run compilation in watch mode, recompiling when files change
pub fn run_watch_mode(source_path: PathBuf) {
    use miette::NamedSource;
    use std::io::Read;

    // Print initial message
    tui::print_success("Starting watch mode...".to_string());

    // Create a channel for file system events
    let (tx, rx) = channel();

    // Create a watcher
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            match res {
                Ok(event) => {
                    // We're interested in write events to .som files
                    if event.kind.is_modify() {
                        for path in event.paths {
                            if path.extension().map_or(false, |ext| ext == "som") {
                                if let Err(e) = tx.send(()) {
                                    eprintln!("Error sending watch event: {}", e);
                                }
                                break;
                            }
                        }
                    }
                }
                Err(e) => eprintln!("Watch error: {:?}", e),
            }
        }) {
            Ok(watcher) => watcher,
            Err(e) => {
                tui::print_error(format!("Failed to create file watcher: {}", e));
                std::process::exit(1);
            }
        };

    // Watch the directory containing the source file
    let watch_dir = source_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    if let Err(e) = watcher.watch(watch_dir, RecursiveMode::NonRecursive) {
        tui::print_error(format!("Failed to watch directory: {}", e));
        std::process::exit(1);
    }

    // Function to compile once
    let compile_once = || {
        // Catch panics during file reading and compilation
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Read the source file
            let mut content = String::new();
            if let Err(e) = std::fs::File::open(&source_path)
                .and_then(|mut file| file.read_to_string(&mut content))
            {
                tui::print_error(format!(
                    "Error reading source file '{}': {}",
                    source_path.display(),
                    e
                ));
                return;
            }

            let name: String = source_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("main")
                .to_string();

            let source = NamedSource::new(name, content);

            // Clear screen before recompiling
            print!("\x1B[2J\x1B[H");

            // Run compilation (this will handle errors internally)
            run_with_process_tree(source);
        }));

        // Handle any panics that occurred during compilation
        if let Err(panic) = result {
            let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
                msg.clone()
            } else if let Some(msg) = panic.downcast_ref::<&str>() {
                msg.to_string()
            } else {
                "Unknown error during compilation".to_string()
            };
            tui::print_error(format!("Compilation process failed: {}", panic_message));
        }
    };

    // Initial compilation
    compile_once();

    // Watch for changes
    loop {
        match rx.recv() {
            Ok(()) => {
                // Debounce: wait a bit to avoid multiple rapid events
                thread::sleep(Duration::from_millis(50));

                // Drain any additional events that might have accumulated
                while rx.try_recv().is_ok() {}

                println!("\nFile changed, recompiling...");
                compile_once();
            }
            Err(e) => {
                tui::print_error(format!("Watch channel error: {}", e));
                break;
            }
        }
    }
}

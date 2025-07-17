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
    // Compilation phase
    let lexer = Lexer::new(source.inner().as_str());

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
    };

    let mut compiler = Compiler::new();
    let compiled = compiler.compile(&type_checked);

    // Execution phase (separate from compilation)
    let runner = Runner::new();
    let return_value = runner.run(compiled).unwrap();

    return_value
}

/// Run compilation with process tree visualization
pub fn run_with_process_tree(source: miette::NamedSource<String>) -> Option<i64> {
    use compilation_result::COMPILED_CODE;

    // Reset the compiled code storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.code = None;
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
                    let code_ptr = {
                        let code_storage = COMPILED_CODE.lock().unwrap();
                        match code_storage.code {
                            Some(ptr) => ptr,
                            None => {
                                tui::print_error("No compiled code available!".to_string());
                                return None;
                            }
                        }
                    };

                    // Execute the compiled code
                    let runner = Runner::new();
                    let return_value = runner.run(code_ptr).unwrap();
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

    // Stage 1: Lexing
    update_stage_note(&process_tree, "lexing");

    let lexer = Lexer::new(source.inner().as_str());

    // Stage 2: parsing
    update_stage_note(&process_tree, "parsing");

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            let mut error_messages = Vec::new();
            for error in errors {
                let report = miette::miette!(error).with_source_code(source.clone());
                error_messages.push(report);
            }
            return Err(error_messages);
        }
    };

    // Stage 3: type checking
    update_stage_note(&process_tree, "type checking");

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            let mut error_messages = Vec::new();
            for error in errors {
                let report = miette::miette!(error).with_source_code(source.clone());
                error_messages.push(report);
            }
            return Err(error_messages);
        }
    };

    // Stage 4: code generation
    update_stage_note(&process_tree, "code generation");

    let mut compiler = Compiler::new();
    let compiled = compiler.compile(&type_checked);

    // Store the compiled code in our global storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.set_code(compiled);
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
        // Read the source file
        let mut content = String::new();
        if let Err(e) =
            std::fs::File::open(&source_path).and_then(|mut file| file.read_to_string(&mut content))
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

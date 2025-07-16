use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use colored::Colorize;

use crate::prelude::*;
use crate::tui::{format_elapsed_time, format_process_name, format_state, Process, ProcessState};

/// Run compilation without process tree visualization
pub fn run(source: miette::NamedSource<String>) -> i64 {
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

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    ran
}

/// Run compilation with process tree visualization
pub fn run_with_process_tree(source: miette::NamedSource<String>) -> i64 {
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
                Ok(value) => {
                    let return_value = *value;
                    drop(compilation_guard); // Release the lock

                    // Update main process to completed
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Completed;
                        tree.note = None;
                        tree.completed_at = Some(SystemTime::now());
                    }

                    let tree = process_tree.lock().unwrap();
                    crate::tui::draw_process_tree_animated(&tree);

                    // Print that the compilation completed
                    eprintln!(
                        "\n  {} compilation {}\n",
                        format_state(&ProcessState::Completed),
                        format_process_name("completed succesfully", &ProcessState::Completed)
                            .bright_green()
                    );

                    return return_value;
                }
                Err(error_reports) => {
                    // Update main process to error
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Error;
                        tree.note = format!("{} errors", error_reports.len()).into();
                        tree.completed_at = Some(SystemTime::now());
                    }

                    // Show error state for a moment
                    let tree = process_tree.lock().unwrap();
                    crate::tui::draw_process_tree_animated(&tree);

                    // Clear screen and print stored error messages immediately
                    //print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top

                    // Print that the compilation failed with x errors
                    eprintln!(
                        "\n  {} compilation {} with {} {}\n",
                        format_state(&ProcessState::Error),
                        format_process_name("failed", &ProcessState::Error).bright_green(),
                        error_reports.len(),
                        if error_reports.len() == 1 {
                            "error"
                        } else {
                            "errors"
                        }
                    );

                    // Print miette errors with proper formatting
                    for error_report in error_reports {
                        eprintln!("{:?}", error_report);
                    }

                    std::process::exit(1);
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
fn run_compilation_stages(
    source: miette::NamedSource<String>,
    process_tree: Arc<Mutex<Process>>,
) -> std::result::Result<i64, Vec<miette::Report>> {
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

    // Stage 5: execution
    update_stage_note(&process_tree, "execution");

    let runner = Runner::new();
    let ran = runner.run(compiled).unwrap();

    Ok(ran)
}

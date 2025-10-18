use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};

use crate::tui;

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
            super::run_with_process_tree(source, Some(source_path.clone()));
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

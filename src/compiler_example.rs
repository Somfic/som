use crate::tui::{Process, ProcessState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

/// Example of integrating the process tree with a compiler
/// This shows how library dependencies would be represented as child processes
pub fn run_compiler_simulation() {
    let now = SystemTime::now();

    // Create a shared process tree that can be updated during compilation
    let process_tree = Arc::new(Mutex::new(create_initial_project_tree(now)));
    let should_stop = Arc::new(AtomicBool::new(false));

    // Clone references for the simulation thread
    let tree_clone = Arc::clone(&process_tree);
    let stop_clone = Arc::clone(&should_stop);

    // Start the compilation simulation in a separate thread
    let simulation_handle = thread::spawn(move || {
        run_compilation_simulation(tree_clone, stop_clone);
    });

    // Display the animated tree
    loop {
        {
            let tree = process_tree.lock().unwrap();
            crate::tui::draw_process_tree_animated(&tree);
        }

        if should_stop.load(Ordering::Relaxed) {
            break;
        }

        thread::sleep(Duration::from_millis(200));
    }

    simulation_handle.join().unwrap();
}

fn create_initial_project_tree(now: SystemTime) -> Process {
    Process {
        name: "my_project".to_string(),
        state: ProcessState::Compiling,
        started_at: now,
        completed_at: None,
        children: vec![
            // Core dependencies
            Process {
                name: "serde".to_string(),
                state: ProcessState::Compiling,
                started_at: now,
                completed_at: None,
                children: vec![
                    Process {
                        name: "serde_derive".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(1),
                        completed_at: None,
                        children: vec![],
                    },
                    Process {
                        name: "proc-macro2".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(1),
                        completed_at: None,
                        children: vec![Process {
                            name: "unicode-ident".to_string(),
                            state: ProcessState::Waiting,
                            started_at: now + Duration::from_secs(2),
                            completed_at: None,
                            children: vec![],
                        }],
                    },
                ],
            },
            // Web framework dependencies
            Process {
                name: "tokio".to_string(),
                state: ProcessState::Waiting,
                started_at: now + Duration::from_secs(2),
                completed_at: None,
                children: vec![
                    Process {
                        name: "tokio-macros".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(3),
                        completed_at: None,
                        children: vec![],
                    },
                    Process {
                        name: "mio".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(3),
                        completed_at: None,
                        children: vec![Process {
                            name: "libc".to_string(),
                            state: ProcessState::Waiting,
                            started_at: now + Duration::from_secs(4),
                            completed_at: None,
                            children: vec![],
                        }],
                    },
                ],
            },
            // HTTP client dependencies
            Process {
                name: "reqwest".to_string(),
                state: ProcessState::Waiting,
                started_at: now + Duration::from_secs(5),
                completed_at: None,
                children: vec![
                    Process {
                        name: "hyper".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(6),
                        completed_at: None,
                        children: vec![
                            Process {
                                name: "http".to_string(),
                                state: ProcessState::Waiting,
                                started_at: now + Duration::from_secs(7),
                                completed_at: None,
                                children: vec![],
                            },
                            Process {
                                name: "httparse".to_string(),
                                state: ProcessState::Waiting,
                                started_at: now + Duration::from_secs(7),
                                completed_at: None,
                                children: vec![],
                            },
                        ],
                    },
                    Process {
                        name: "url".to_string(),
                        state: ProcessState::Waiting,
                        started_at: now + Duration::from_secs(6),
                        completed_at: None,
                        children: vec![Process {
                            name: "idna".to_string(),
                            state: ProcessState::Waiting,
                            started_at: now + Duration::from_secs(8),
                            completed_at: None,
                            children: vec![],
                        }],
                    },
                ],
            },
        ],
    }
}

fn run_compilation_simulation(process_tree: Arc<Mutex<Process>>, should_stop: Arc<AtomicBool>) {
    let compilation_steps = vec![
        // Step 1: Start with basic dependencies
        (2.0, "unicode-ident", ProcessState::Compiling),
        (3.0, "libc", ProcessState::Compiling),
        (4.0, "unicode-ident", ProcessState::Completed),
        (5.0, "proc-macro2", ProcessState::Compiling),
        (6.0, "libc", ProcessState::Completed),
        (7.0, "serde_derive", ProcessState::Compiling),
        (8.0, "proc-macro2", ProcessState::Completed),
        (9.0, "serde_derive", ProcessState::Completed),
        (10.0, "serde", ProcessState::Completed),
        // Step 2: Tokio compilation
        (11.0, "mio", ProcessState::Compiling),
        (12.0, "tokio-macros", ProcessState::Compiling),
        (13.0, "tokio-macros", ProcessState::Completed),
        (14.0, "mio", ProcessState::Completed),
        (15.0, "tokio", ProcessState::Compiling),
        (16.0, "tokio", ProcessState::Completed),
        // Step 3: HTTP dependencies
        (17.0, "http", ProcessState::Compiling),
        (18.0, "httparse", ProcessState::Compiling),
        (19.0, "idna", ProcessState::Compiling),
        (20.0, "http", ProcessState::Completed),
        (21.0, "httparse", ProcessState::Completed),
        (22.0, "idna", ProcessState::Completed),
        (23.0, "url", ProcessState::Compiling),
        (24.0, "hyper", ProcessState::Compiling),
        (25.0, "url", ProcessState::Completed),
        (26.0, "hyper", ProcessState::Completed),
        (27.0, "reqwest", ProcessState::Compiling),
        (28.0, "reqwest", ProcessState::Completed),
        // Step 4: Main project compilation
        (29.0, "my_project", ProcessState::Completed),
    ];

    let start_time = SystemTime::now();

    for (delay_seconds, lib_name, new_state) in compilation_steps {
        thread::sleep(Duration::from_millis((delay_seconds * 1000.0) as u64));

        let mut tree = process_tree.lock().unwrap();
        let current_time = SystemTime::now();

        update_process_state(&mut tree, lib_name, new_state, current_time);
    }

    // Let it run for a bit more to show the final state
    thread::sleep(Duration::from_secs(5));
    should_stop.store(true, Ordering::Relaxed);
}

fn update_process_state(
    process: &mut Process,
    name: &str,
    new_state: ProcessState,
    current_time: SystemTime,
) {
    if process.name == name {
        process.state = new_state.clone();
        if matches!(new_state, ProcessState::Completed | ProcessState::Error) {
            process.completed_at = Some(current_time);
        }
        return;
    }

    for child in &mut process.children {
        update_process_state(child, name, new_state.clone(), current_time);
    }
}

/// Alternative example showing a failed compilation with error states
pub fn run_failed_compilation_example() {
    let now = SystemTime::now();

    let process_tree = Process {
        name: "web_server".to_string(),
        state: ProcessState::Error,
        started_at: now - Duration::from_secs(45),
        completed_at: Some(now - Duration::from_secs(5)),
        children: vec![
            Process {
                name: "actix-web".to_string(),
                state: ProcessState::Completed,
                started_at: now - Duration::from_secs(40),
                completed_at: Some(now - Duration::from_secs(20)),
                children: vec![
                    Process {
                        name: "actix-rt".to_string(),
                        state: ProcessState::Completed,
                        started_at: now - Duration::from_secs(38),
                        completed_at: Some(now - Duration::from_secs(30)),
                        children: vec![],
                    },
                    Process {
                        name: "futures-util".to_string(),
                        state: ProcessState::Completed,
                        started_at: now - Duration::from_secs(35),
                        completed_at: Some(now - Duration::from_secs(25)),
                        children: vec![],
                    },
                ],
            },
            Process {
                name: "sqlx".to_string(),
                state: ProcessState::Error,
                started_at: now - Duration::from_secs(30),
                completed_at: Some(now - Duration::from_secs(10)),
                children: vec![
                    Process {
                        name: "sqlx-core".to_string(),
                        state: ProcessState::Completed,
                        started_at: now - Duration::from_secs(28),
                        completed_at: Some(now - Duration::from_secs(15)),
                        children: vec![],
                    },
                    Process {
                        name: "sqlx-macros".to_string(),
                        state: ProcessState::Error,
                        started_at: now - Duration::from_secs(25),
                        completed_at: Some(now - Duration::from_secs(10)),
                        children: vec![Process {
                            name: "syn".to_string(),
                            state: ProcessState::Completed,
                            started_at: now - Duration::from_secs(23),
                            completed_at: Some(now - Duration::from_secs(18)),
                            children: vec![],
                        }],
                    },
                ],
            },
            Process {
                name: "config".to_string(),
                state: ProcessState::Waiting,
                started_at: now - Duration::from_secs(20),
                completed_at: None,
                children: vec![Process {
                    name: "serde_yaml".to_string(),
                    state: ProcessState::Waiting,
                    started_at: now - Duration::from_secs(18),
                    completed_at: None,
                    children: vec![],
                }],
            },
        ],
    };

    crate::tui::start_animated_display(process_tree);
}

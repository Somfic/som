mod compilation_result;
mod process_tree;
mod simple;
mod watch;

// Re-export public functions
pub use process_tree::run_with_process_tree;
pub use simple::run;
pub use watch::run_watch_mode;

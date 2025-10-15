use colored::*;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

static ANIMATION_FRAME: AtomicU32 = AtomicU32::new(0);

pub fn print_error(message: impl Into<String>) {
    eprintln!(
        "  {} {}",
        format_state(&ProcessState::Error),
        message.into()
    );
}

pub fn print_success(message: impl Into<String>) {
    println!(
        "  {} {}",
        format_state(&ProcessState::Completed),
        message.into()
    );
}

#[derive(Debug, Clone)]
pub enum ProcessState {
    Running,
    Waiting,
    Error,
    Completed,
}

impl ProcessState {
    pub fn apply_bright_color(&self, text: &str) -> String {
        match self {
            ProcessState::Running => text.bright_blue().to_string(),
            ProcessState::Waiting => text.bright_black().to_string(),
            ProcessState::Error => text.bright_red().to_string(),
            ProcessState::Completed => text.bright_green().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Process {
    pub name: String,
    pub state: ProcessState,
    pub note: Option<String>,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub children: Vec<Process>,
}

pub fn draw_process_tree(process: &Process) {
    // Draw children with tree structure first (in reverse order)
    // Filter out waiting subtrees to reduce clutter
    let active_children: Vec<&Process> = process
        .children
        .iter()
        .filter(|child| !is_process_and_children_waiting(child))
        .collect();

    for (i, child) in active_children.iter().enumerate().rev() {
        let is_last_child = i == active_children.len() - 1;
        draw_process_tree_with_prefix(child, String::new(), is_last_child);
    }

    // Draw root process at the bottom without tree connector
    let elapsed_time = format_elapsed_time(
        process.started_at,
        process.completed_at,
        &process.state,
        None,
    );
    let elapsed_time = if elapsed_time.is_empty() {
        String::new()
    } else {
        format!(" ◦ {}", elapsed_time).bright_black().to_string()
    };

    let note_text = process
        .note
        .as_ref()
        .map(|n| format!(" {}", n.bright_black().italic()))
        .unwrap_or_default();

    println!(
        "  {} {} {}{}{}\x1b[K",
        "◦".bright_black(),
        format_process_name(&process.name, &process.state),
        format_state(&process.state),
        note_text,
        elapsed_time
    );
}

fn draw_process_tree_with_prefix(process: &Process, prefix: String, is_last: bool) {
    // Draw children first (in reverse order)
    // Filter out waiting subtrees to reduce clutter
    let active_children: Vec<&Process> = process
        .children
        .iter()
        .filter(|child| !is_process_and_children_waiting(child))
        .collect();

    for (i, child) in active_children.iter().enumerate().rev() {
        let is_last_child = i == active_children.len() - 1;
        // Continue the vertical line only if this process is not the last sibling
        let child_prefix = if is_last {
            format!("{}  ", prefix)
        } else {
            format!("{}{} ", prefix, "│".bright_black())
        };
        draw_process_tree_with_prefix(child, child_prefix.clone(), is_last_child);
    }

    // Draw the current process with prettier rounded connectors
    let connector = if is_last {
        "╭─◦ ".bright_black().to_string()
    } else {
        "├─◦ ".bright_black().to_string()
    };

    let elapsed_time = format_elapsed_time(
        process.started_at,
        process.completed_at,
        &process.state,
        Some(10),
    );
    let elapsed_time = if elapsed_time.is_empty() {
        String::new()
    } else {
        format!(" ◦ {}", elapsed_time).bright_black().to_string()
    };

    // Only show note text if the process is not completed
    let note_text = if matches!(process.state, ProcessState::Completed) {
        String::new()
    } else {
        process
            .note
            .as_ref()
            .map(|n| format!("{}", n.bright_black().italic()))
            .unwrap_or_default()
    };

    println!(
        "  {}{}{} {}{}{}\x1b[K",
        prefix,
        connector,
        format_process_name(&process.name, &process.state),
        format_state(&process.state),
        note_text,
        elapsed_time
    );
}

pub fn format_process_name(name: &str, state: &ProcessState) -> String {
    match state {
        ProcessState::Running => state.apply_bright_color(name),
        ProcessState::Waiting => state.apply_bright_color(name),
        ProcessState::Error => state.apply_bright_color(name),
        ProcessState::Completed => state.apply_bright_color(name),
    }
}

pub fn format_state(state: &ProcessState) -> String {
    let frame = ANIMATION_FRAME.load(Ordering::Relaxed);

    match state {
        ProcessState::Running => {
            let spinner_chars = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let spinner = spinner_chars[frame as usize % spinner_chars.len()];
            let text = format!("{} ", spinner);
            state.apply_bright_color(&text)
        }
        ProcessState::Waiting => {
            let stop_chars = ["◦", "◇", "◦", "◇"];
            let stop = stop_chars[frame as usize % stop_chars.len()];
            let text = format!("{}", stop);
            state.apply_bright_color(&text)
        }
        ProcessState::Error => {
            let text = format!("×");
            state.apply_bright_color(&text)
        }
        ProcessState::Completed => {
            let text = format!("✓");
            state.apply_bright_color(&text)
        }
    }
}

pub fn format_elapsed_time(
    started_at: SystemTime,
    completed_at: Option<SystemTime>,
    state: &ProcessState,
    threshold: Option<u64>,
) -> String {
    let end_time = match state {
        ProcessState::Completed | ProcessState::Error => {
            completed_at.unwrap_or_else(|| SystemTime::now())
        }
        _ => SystemTime::now(),
    };

    let elapsed = end_time
        .duration_since(started_at)
        .unwrap_or(Duration::from_secs(0));

    let seconds = elapsed.as_secs();
    let milliseconds = elapsed.subsec_millis();

    // Only show elapsed time if it's been longer than 10 seconds
    if threshold.is_some() && seconds < threshold.unwrap() as u64 {
        return String::new();
    }

    if seconds < 60 {
        format!("{}.{:1}s", seconds, milliseconds)
    } else if seconds < 3600 {
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        format!("{}m {}.{:1}s", minutes, remaining_seconds, milliseconds)
    } else {
        let hours = seconds / 3600;
        let remaining_minutes = (seconds % 3600) / 60;
        let remaining_seconds = seconds % 60;
        format!(
            "{}h {}m {}.{:1}s",
            hours, remaining_minutes, remaining_seconds, milliseconds
        )
    }
}

pub fn draw_process_tree_animated(process: &Process) {
    // Move cursor to top-left without clearing the screen
    print!("\x1b[H");

    // Draw the tree
    draw_process_tree(process);

    // Clear any remaining lines from previous output
    print!("\x1b[J");

    // Flush output to ensure it's written immediately
    use std::io::{self, Write};
    io::stdout().flush().unwrap();

    // Increment animation frame
    ANIMATION_FRAME.fetch_add(1, Ordering::Relaxed);
}

// Helper function to check if a process and all its descendants are in waiting state
fn is_process_and_children_waiting(process: &Process) -> bool {
    if !matches!(process.state, ProcessState::Waiting) {
        return false;
    }

    // Check if all children are also waiting (recursively)
    process
        .children
        .iter()
        .all(|child| is_process_and_children_waiting(child))
}

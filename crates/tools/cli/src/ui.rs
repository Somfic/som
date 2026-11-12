//! Everything the CLI tells the user goes through here, so it always looks the same:
//!
//! every message is a [`Diagnostic`], so it has the same shape whether things went
//! well or not, and whether or not it points into source code:
//!
//! ```text
//! wrote: example/example.c
//! built: example/example
//! error: example does not exist
//! ```
//!
//! Colour is used only when stderr is a terminal and `NO_COLOR` isn't set.

use som_base::{Diagnostic, Message};
use std::io::IsTerminal;
use std::sync::OnceLock;

static COLOR: OnceLock<bool> = OnceLock::new();

fn color() -> bool {
    *COLOR.get_or_init(|| {
        std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
    })
}

/// Reports something that went well: `success("built", path)` shows `built: <path>`.
pub fn success(what: &'static str, subject: impl Into<Message>) {
    diagnostic(&Diagnostic::success(what, subject));
}

/// Shows a diagnostic. Ones with code snippets are followed by a blank line,
/// to keep consecutive ones apart.
pub fn diagnostic(diagnostic: &Diagnostic) {
    let mut text = diagnostic.render(color());
    if !diagnostic.labels.is_empty() {
        text.push('\n');
    }
    eprint!("{text}");
}

/// Shows a problem that has no place in the source code.
pub fn error(message: impl Into<Message>) {
    diagnostic(&Diagnostic::error(message));
}

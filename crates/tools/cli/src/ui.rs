//! Everything the CLI tells the user is a [`Diagnostic`], so it has the same shape
//! whether things went well or not, and whether or not it points into source code.

use som_base::diagnostics::RenderOptions;
use som_base::{Diagnostic, Message};
use std::io::IsTerminal;
use std::sync::OnceLock;

static COLOR: OnceLock<bool> = OnceLock::new();

fn color() -> bool {
    *COLOR.get_or_init(|| {
        std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
    })
}

/// `success("built", path)` shows `built: <path>`.
pub fn success(what: &'static str, subject: impl Into<Message>) {
    diagnostic(&Diagnostic::success(what, subject));
}

/// Ones with code snippets are followed by a blank line, to keep consecutive ones apart.
pub fn diagnostic(diagnostic: &Diagnostic) {
    let mut text = diagnostic.render_with(RenderOptions {
        color: color(),
        highlight: Some(som_driver::highlight),
    });
    if !diagnostic.labels.is_empty() {
        text.push('\n');
    }
    eprint!("{text}");
}

pub fn error(message: impl Into<Message>) {
    diagnostic(&Diagnostic::error(message));
}

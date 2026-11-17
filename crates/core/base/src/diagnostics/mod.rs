use owo_colors::OwoColorize;
use std::fmt::Display;
use std::sync::Arc;

use crate::message::Message;
use crate::{Source, Span};

mod palette;
mod render;
mod text;

pub use palette::*;
pub use render::{Highlighter, RenderOptions};
pub use text::{closest_match, edit_distance, pascal_case, snake_case, strip_ansi};

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Lines from elsewhere, like a C compiler's output, shown as they are.
    pub trace: Vec<String>,
    pub message: Message,
    pub hints: Vec<Message>,
    pub labels: Vec<Label>,
    pub fixes: Vec<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub title: Message,
    /// An empty span inserts.
    pub edits: Vec<(Span, String)>,
}

impl Fix {
    pub fn change_to(span: Span, text: impl Into<String>) -> Fix {
        let text = text.into();
        Fix::replace(
            crate::msg!["change to ", crate::message::code(&text)],
            span,
            text,
        )
    }

    pub fn replace(title: impl Into<Message>, span: Span, text: impl Into<String>) -> Fix {
        Fix {
            title: title.into(),
            edits: vec![(span, text.into())],
        }
    }

    pub fn add_line(title: impl Into<Message>, source: Arc<Source>, line: &str) -> Fix {
        Fix {
            title: title.into(),
            edits: vec![(Span::from_range(0..0, source), format!("{line}\n"))],
        }
    }
}

impl Diagnostic {
    fn new(severity: Severity, message: impl Into<Message>) -> Self {
        Self {
            severity,
            trace: vec![],
            message: message.into(),
            hints: vec![],
            labels: vec![],
            fixes: vec![],
        }
    }

    pub fn error(message: impl Into<Message>) -> Self {
        Self::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<Message>) -> Self {
        Self::new(Severity::Warning, message)
    }

    pub fn hint(message: impl Into<Message>) -> Self {
        Self::new(Severity::Hint, message)
    }

    pub fn success(what: &'static str, message: impl Into<Message>) -> Self {
        Self::new(Severity::Success(what), message)
    }

    pub fn note(message: impl Into<Message>) -> Self {
        Self::new(Severity::Note, message)
    }

    pub fn with_label(mut self, label: impl Into<Label>) -> Self {
        self.labels.push(label.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<Message>) -> Self {
        self.hints.push(hint.into());
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fixes.push(fix);
        self
    }

    pub fn with_trace(mut self, trace: impl Into<String>) -> Self {
        self.trace.push(trace.into());
        self
    }
}

#[derive(Debug, Clone)]
pub enum Severity {
    Error,
    Warning,
    Note,
    /// Nothing's wrong, but it could be better: `fib(20)` could be `@fib(20)`.
    Hint,
    /// Labelled with what happened, like `wrote` or `built`.
    Success(&'static str),
}

impl Severity {
    fn color(&self) -> Rgb {
        match self {
            Severity::Error => RED,
            Severity::Warning => YELLOW,
            Severity::Note | Severity::Hint => BLUE,
            Severity::Success(_) => GREEN,
        }
    }
}

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = self.color();
        write!(
            f,
            "{}",
            match self {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
                Severity::Hint => "hint",
                Severity::Success(what) => what,
            }
            .truecolor(c.0, c.1, c.2)
            .bold()
        )
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub message: Message,
    pub span: Span,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<Message>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: true,
        }
    }

    pub fn secondary(span: Span, message: impl Into<Message>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: false,
        }
    }
}

impl Span {
    pub fn label(&self, message: impl Into<Message>) -> Label {
        Label {
            message: message.into(),
            span: self.clone(),
            is_primary: true,
        }
    }
}

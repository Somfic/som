//! A diagnostic's text: plain words mixed with tagged parts of the program, each shown in
//! its theme colour in a terminal, or between backticks without colour.

use owo_colors::{OwoColorize, Style};
use std::fmt::Display;

use crate::diagnostics::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message(pub Vec<Part>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Text(String),
    /// Syntax-highlighted som.
    Code(String),
    Type(String),
    Name(Kind, String),
    Path(String),
    /// A shell command, flag or environment variable.
    Command(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Var,
    Func,
    Struct,
    Field,
    Keyword,
    Module,
}

/// A [`Message`] from parts, strings (plain text) and other messages.
#[macro_export]
macro_rules! msg {
    ($($part:expr),* $(,)?) => {{
        #[allow(unused_mut)]
        let mut message = $crate::message::Message::default();
        $(message.push($part);)*
        message
    }};
}

pub fn code(s: impl Display) -> Part {
    Part::Code(s.to_string())
}

pub fn ty(s: impl Display) -> Part {
    Part::Type(s.to_string())
}

pub fn path(s: impl Display) -> Part {
    Part::Path(s.to_string())
}

pub fn command(s: impl Display) -> Part {
    Part::Command(s.to_string())
}

pub fn var(s: impl Display) -> Part {
    Part::Name(Kind::Var, s.to_string())
}

pub fn func(s: impl Display) -> Part {
    Part::Name(Kind::Func, s.to_string())
}

pub fn struct_name(s: impl Display) -> Part {
    Part::Name(Kind::Struct, s.to_string())
}

pub fn field(s: impl Display) -> Part {
    Part::Name(Kind::Field, s.to_string())
}

pub fn keyword(s: impl Display) -> Part {
    Part::Name(Kind::Keyword, s.to_string())
}

pub fn module(s: impl Display) -> Part {
    Part::Name(Kind::Module, s.to_string())
}

impl From<&str> for Part {
    fn from(s: &str) -> Self {
        Part::Text(s.to_string())
    }
}

impl From<String> for Part {
    fn from(s: String) -> Self {
        Part::Text(s)
    }
}

impl From<&String> for Part {
    fn from(s: &String) -> Self {
        Part::Text(s.clone())
    }
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        Message(vec![s.into()])
    }
}

impl From<String> for Message {
    fn from(s: String) -> Self {
        Message(vec![s.into()])
    }
}

impl From<&String> for Message {
    fn from(s: &String) -> Self {
        Message(vec![s.into()])
    }
}

impl From<Part> for Message {
    fn from(part: Part) -> Self {
        Message(vec![part])
    }
}

impl Message {
    pub fn push(&mut self, more: impl Into<Message>) {
        self.0.extend(more.into().0);
    }

    pub fn plain(&self) -> String {
        self.render(false)
    }

    pub fn render(&self, color: bool) -> String {
        self.render_with(RenderOptions {
            color,
            highlight: None,
        })
    }

    pub fn render_with(&self, options: RenderOptions) -> String {
        let color = options.color;
        let mut out = String::new();
        for part in &self.0 {
            match part {
                Part::Text(s) => out.push_str(s),
                _ if !color => {
                    out.push('`');
                    out.push_str(part.content());
                    out.push('`');
                }
                Part::Code(s) => out.push_str(&options.highlight(s)),
                Part::Type(s) => out.push_str(&paint(s, YELLOW, Style::new())),
                Part::Path(s) => out.push_str(&paint(s, LAVENDER, Style::new())),
                Part::Command(s) => out.push_str(&paint(s, SKY, Style::new())),
                Part::Name(kind, s) => out.push_str(&match kind {
                    Kind::Var => paint(s, TEXT, Style::new().bold()),
                    Kind::Func => paint(s, BLUE, Style::new()),
                    Kind::Struct => paint(s, PEACH, Style::new()),
                    Kind::Field => paint(s, TEAL, Style::new()),
                    Kind::Keyword => paint(s, MAUVE, Style::new().italic()),
                    Kind::Module => paint(s, GREEN, Style::new()),
                }),
            }
        }
        out
    }

    /// `a`, `a and b`, `a, b and c`, with `last` for the final `and`.
    pub fn list(items: impl IntoIterator<Item = Part>, last: &str) -> Message {
        let items: Vec<Part> = items.into_iter().collect();
        let mut out = Message(Vec::new());
        let count = items.len();
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                out.0.push(if i + 1 == count { last } else { ", " }.into());
            }
            out.0.push(item);
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|p| p.content().is_empty())
    }
}

impl Part {
    fn content(&self) -> &str {
        match self {
            Part::Text(s)
            | Part::Code(s)
            | Part::Type(s)
            | Part::Path(s)
            | Part::Command(s)
            | Part::Name(_, s) => s,
        }
    }
}

fn paint(s: &str, color: Rgb, style: Style) -> String {
    s.style(style.truecolor(color.0, color.1, color.2))
        .to_string()
}

impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.plain())
    }
}

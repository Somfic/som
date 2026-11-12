//! Text for diagnostics that knows what it's talking about.
//!
//! A [`Message`] is a list of parts: plain words, and things from the program,
//! like a type, a function's name, a file, or a snippet of som. Each part is
//! shown in its theme colour in a terminal, or between backticks without colour:
//!
//! ```text
//! expected i32, found bool        (in colour: i32 and bool in the type colour)
//! expected `i32`, found `bool`    (without)
//! ```
//!
//! Build one with [`msg!`](crate::msg):
//!
//! ```
//! use som_base::msg;
//! use som_base::message::{path, ty};
//!
//! let m = msg!["expected ", ty("i32"), ", found ", ty("bool")];
//! assert_eq!(m.plain(), "expected `i32`, found `bool`");
//! ```

use owo_colors::{OwoColorize, Style};
use std::fmt::Display;

use crate::diagnostics::*;

/// A diagnostic's text: plain words mixed with tagged parts of the program.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Message(pub Vec<Part>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    /// Plain words.
    Text(String),
    /// A snippet of som, syntax-highlighted: `use std;`
    Code(String),
    /// A type: `i32`, `&Point`
    Type(String),
    /// A name from the program.
    Name(Kind, String),
    /// A file or directory: `example.som`
    Path(String),
    /// Something to type in a shell: a command, flag or environment variable: `--cc`
    Command(String),
}

/// What a [`Part::Name`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Var,
    Func,
    Struct,
    Field,
    Keyword,
    Module,
}

/// Builds a [`Message`] from parts, strings (plain text) and other messages.
///
/// `msg!["cannot find ", func("helper")]`
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
    /// Appends a part, some text, or a whole message.
    pub fn push(&mut self, more: impl Into<Message>) {
        self.0.extend(more.into().0);
    }

    /// The text with every tagged part between backticks, and no colour.
    pub fn plain(&self) -> String {
        self.render(false)
    }

    /// The text in theme colours, or as [`plain`](Self::plain) text when `color` is false.
    pub fn render(&self, color: bool) -> String {
        let mut out = String::new();
        for part in &self.0 {
            match part {
                Part::Text(s) => out.push_str(s),
                _ if !color => {
                    out.push('`');
                    out.push_str(part.content());
                    out.push('`');
                }
                Part::Code(s) => out.push_str(&syntax_highlight(s)),
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

/// Plain text, so a message can be used anywhere a string can.
impl Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.plain())
    }
}

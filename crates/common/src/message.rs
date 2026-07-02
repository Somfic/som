//! Structured diagnostic messages: a sequence of parts that are either plain
//! prose or `Code` fragments. The renderer syntax-highlights the code parts, so
//! a message like `expected i32, found bool` needs no backticks — the type
//! names are marked as code and styled automatically.
//!
//! Build one with the [`message!`] macro, mixing string literals (prose) with
//! anything that is [`IntoMessagePart`] (e.g. a `TokenKind`) or wrapped in
//! [`code`]:
//!
//! ```ignore
//! message!["expected ", TokenKind::Semicolon, ", found ", token.kind]
//! message!["type mismatch: expected ", code(want), ", found ", code(got)]
//! ```

#[derive(Debug, Clone)]
pub enum MessagePart {
    Text(String),
    Code(String),
}

#[derive(Debug, Clone, Default)]
pub struct Message {
    pub parts: Vec<MessagePart>,
}

impl Message {
    /// The message with all styling stripped — for width math, plain-text
    /// sinks (e.g. an LSP), or `contains` checks in tests.
    pub fn plain(&self) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                MessagePart::Text(s) | MessagePart::Code(s) => s.as_str(),
            })
            .collect()
    }
}

/// Convert a value into a single [`MessagePart`]. Strings become prose; types
/// that represent source constructs (like `TokenKind`) implement this to become
/// `Code`.
pub trait IntoMessagePart {
    fn into_message_part(self) -> MessagePart;
}

impl IntoMessagePart for MessagePart {
    fn into_message_part(self) -> MessagePart {
        self
    }
}

impl IntoMessagePart for &str {
    fn into_message_part(self) -> MessagePart {
        MessagePart::Text(self.to_string())
    }
}

impl IntoMessagePart for String {
    fn into_message_part(self) -> MessagePart {
        MessagePart::Text(self)
    }
}

/// Mark any `Display` value as a code fragment, e.g. a resolved type name.
pub fn code(value: impl std::fmt::Display) -> MessagePart {
    MessagePart::Code(value.to_string())
}

impl From<&str> for Message {
    fn from(s: &str) -> Self {
        Message {
            parts: vec![MessagePart::Text(s.to_string())],
        }
    }
}

impl From<String> for Message {
    fn from(s: String) -> Self {
        Message {
            parts: vec![MessagePart::Text(s)],
        }
    }
}

impl From<MessagePart> for Message {
    fn from(part: MessagePart) -> Self {
        Message { parts: vec![part] }
    }
}

/// Build a [`Message`] from a mix of prose and code parts. Each argument is
/// converted with [`IntoMessagePart`].
#[macro_export]
macro_rules! message {
    ($($part:expr),+ $(,)?) => {
        $crate::Message {
            parts: vec![ $($crate::IntoMessagePart::into_message_part($part)),+ ],
        }
    };
}

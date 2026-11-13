//! Infrastructure shared by every part of the compiler.

pub mod arena;
pub mod diagnostics;
pub mod message;
mod span;

pub use arena::{Arena, Id};
pub use diagnostics::{Diagnostic, Fix, Label, Severity};
pub use message::Message;
pub use span::{Position, Source, Span};

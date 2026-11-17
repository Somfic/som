//! The answers to the editor's requests, worked out from an analysis of the program.

pub(crate) mod completion;
pub(crate) mod definition;
pub(crate) mod hover;
pub(crate) mod inlay;
pub(crate) mod lens;
pub(crate) mod rename;
pub(crate) mod semantic_tokens;
pub(crate) mod signature_help;
pub(crate) mod symbols;

/// Where a request points: the file, its text, and the byte offset of the position.
pub(crate) struct Cursor {
    pub file: String,
    pub text: String,
    pub offset: usize,
}

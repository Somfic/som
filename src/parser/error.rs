use crate::diagnostics::{Diagnostic, Severity};
use crate::lexer::TokenKind;
use crate::span::Span;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub expected: Vec<TokenKind>,
    pub found: TokenKind,
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn with_message(message: String, span: Span) -> Self {
        Self {
            expected: vec![],
            found: TokenKind::Eof,
            message,
            span,
        }
    }

    pub fn new(expected: Vec<TokenKind>, found: TokenKind, span: Span) -> Self {
        let expected_str = if expected.is_empty() {
            "nothing".to_string()
        } else if expected.len() == 1 {
            format!("{:?}", expected[0])
        } else {
            format!(
                "one of [{}]",
                expected
                    .iter()
                    .map(|s| format!("{:?}", s))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };

        let message = format!("Expected {}, found {:?}", expected_str, found);

        Self {
            expected,
            found,
            message,
            span,
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        use crate::diagnostics::Label;
        Diagnostic::new(Severity::Error, &self.message)
            .with_label(Label::primary(self.span.clone(), "here"))
    }
}

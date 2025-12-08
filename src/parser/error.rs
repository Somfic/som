use crate::lexer::Syntax;
use crate::span::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub expected: Vec<Syntax>,
    pub found: Syntax,
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub fn new(expected: Vec<Syntax>, found: Syntax, span: Span) -> Self {
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
}

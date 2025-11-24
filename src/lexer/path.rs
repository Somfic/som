use crate::lexer::{TokenKind, TokenValue};
use crate::ParserError;
use crate::{lexer::Identifier, Parse, Parser, Result, Span};
use std::fmt::Display;
use std::hash::Hash;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Path {
    pub segments: Vec<String>,
    pub span: Span,
}

impl Eq for Path {}

impl PartialEq for Path {
    fn eq(&self, other: &Self) -> bool {
        self.segments == other.segments
    }
}

impl Hash for Path {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for segment in &self.segments {
            segment.hash(state);
        }
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<String> = self.segments.iter().map(|s| s.to_string()).collect();
        write!(f, "{}", names.join("::"))
    }
}

impl From<&PathBuf> for Path {
    fn from(path_buf: &PathBuf) -> Self {
        let segments: Vec<String> = path_buf
            .components()
            .filter_map(|comp| {
                if let std::path::Component::Normal(os_str) = comp {
                    os_str.to_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();

        Path {
            span: Span::empty(),
            segments,
        }
    }
}

impl From<&Path> for PathBuf {
    fn from(path: &Path) -> Self {
        let mut path_buf = PathBuf::new();
        for segment in &path.segments {
            path_buf.push(segment);
        }
        path_buf
    }
}

impl Parse for Path {
    type Params = ();

    fn parse(parser: &mut Parser, params: Self::Params) -> Result<Self> {
        let mut segments = Vec::new();
        let mut span: Span = (&parser.lexer.cursor).into();

        loop {
            let ident = parser.expect(
                TokenKind::Identifier,
                "a path segment",
                ParserError::ExpectedSegment,
            )?;

            span = span + ident.span.clone();

            let ident = match ident.value {
                TokenValue::Identifier(id) => id.name.to_string(),
                _ => unreachable!(),
            };

            segments.push(ident);

            if match parser.peek() {
                Some(token) if token.kind == TokenKind::DoubleColon => true,
                _ => false,
            } {
                parser.next();
            } else {
                break;
            }
        }

        Ok(Path { span, segments })
    }
}

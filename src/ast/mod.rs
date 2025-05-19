mod expression;
mod module;
mod span;
mod statement;
mod typing;
use std::fmt::Display;

pub use expression::*;
use miette::{SourceOffset, SourceSpan};
pub use module::*;
pub use span::*;
use span_derive::Span;
pub use statement::*;
pub use typing::*;

use crate::tokenizer::{Token, TokenValue};
use crate::Result;

#[derive(Debug, Clone, Span, Eq)]
pub struct Identifier {
    pub name: Box<str>,
    pub span: miette::SourceSpan,
}

impl std::hash::Hash for Identifier {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Identifier {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl Identifier {
    pub fn new(name: impl Into<String>) -> Self {
        Identifier {
            name: name.into().into(),
            span: SourceSpan::new(SourceOffset::from_location("", 0, 0), 0),
        }
    }

    pub fn from_token(token: &Token) -> Result<Self> {
        let name = match &token.value {
            TokenValue::Identifier(name) => name,
            _ => {
                return Err(vec![miette::diagnostic!(
                    labels = vec![token.label("expected this to be an identifier")],
                    "expected an identifier",
                )])
            }
        };

        Ok(Identifier {
            name: name.name.clone(),
            span: token.span,
        })
    }

    pub fn from_expression(expression: &Expression) -> Result<Identifier> {
        match &expression.value {
            ExpressionValue::Primitive(Primitive::Identifier(identifier)) => Ok(identifier.clone()),
            _ => Err(vec![miette::diagnostic!(
                labels = vec![expression.label("expected this to be an identifier")],
                "expected an identifier",
            )]),
        }
    }
}

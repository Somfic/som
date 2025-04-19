mod expression;
mod module;
mod span;
mod statement;
mod typing;
use std::borrow::Cow;
use std::fmt::Display;

pub use expression::*;
use miette::{SourceOffset, SourceSpan};
pub use module::*;
pub use span::*;
use span_derive::Span;
pub use statement::*;
use syn::token;
pub use typing::*;

use crate::tokenizer::{Token, TokenKind, TokenValue};
use crate::ParserResult;

#[derive(Debug, Clone, Span, Eq, Hash)]
pub struct Identifier<'ast> {
    pub name: Cow<'ast, str>,
    pub span: miette::SourceSpan,
}

impl PartialEq for Identifier<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Identifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl<'ast> Identifier<'ast> {
    pub fn new(name: impl Into<String>) -> Self {
        Identifier {
            name: Cow::Owned(name.into()),
            span: SourceSpan::new(SourceOffset::from_location("", 0, 0), 0),
        }
    }

    pub fn from_token(token: &Token<'ast>) -> ParserResult<Self> {
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

    pub fn from_expression(expression: &Expression<'ast>) -> ParserResult<Identifier<'ast>> {
        match &expression.value {
            ExpressionValue::Primitive(Primitive::Identifier(identifier)) => Ok(identifier.clone()),
            _ => Err(vec![miette::diagnostic!(
                labels = vec![expression.label("expected this to be an identifier")],
                "expected an identifier",
            )]),
        }
    }
}

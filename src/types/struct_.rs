use crate::prelude::*;
use std::hash::Hash;

#[derive(Debug, Clone, Eq)]
pub struct Field {
    pub identifier: Identifier,
    pub type_: Box<Type>,
    pub span: Span,
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.type_ == other.type_
    }
}

impl Hash for Field {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.type_.hash(state);
    }
}

impl From<Field> for Span {
    fn from(field: Field) -> Self {
        field.span
    }
}

impl From<&Field> for Span {
    fn from(field: &Field) -> Self {
        field.span
    }
}

impl From<Field> for miette::SourceSpan {
    fn from(field: Field) -> Self {
        field.span.into()
    }
}

impl From<&Field> for miette::SourceSpan {
    fn from(field: &Field) -> Self {
        field.span.into()
    }
}

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let start = parser.expect(TokenKind::CurlyOpen, "expected a struct type")?;

    let mut fields = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        }) {
            break;
        }

        if !fields.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between fields")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Tilde,
            format!("expected a type for `{}`", identifier.name),
        )?;

        let type_ = parser.parse_type(BindingPower::None)?;

        fields.push(Field {
            span: identifier.span + type_.span,
            identifier,
            type_: Box::new(type_),
        });
    }

    let end = parser.expect(TokenKind::CurlyClose, "expected closing brace for struct")?;

    Ok(TypeValue::Struct(StructType {
        fields,
        span: start.span + end.span,
    })
    .with_span(start.span + end.span))
}

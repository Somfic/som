use crate::{
    ast::{Type, TypeKind},
    lexer::TokenKind,
    parser::{Parse, Parser},
    ParserError, Result,
};

impl Parse for Type {
    type Params = ();

    fn parse(input: &mut Parser, _params: Self::Params) -> Result<Self> {
        let token = input.next()?;

        let kind = match token.kind {
            TokenKind::UnitType => TypeKind::Unit,
            TokenKind::BooleanType => TypeKind::Boolean,
            TokenKind::I32Type => TypeKind::I32,
            TokenKind::I64Type => TypeKind::I64,
            TokenKind::DecimalType => TypeKind::Decimal,
            TokenKind::StringType => TypeKind::String,
            TokenKind::CharacterType => TypeKind::Character,
            _ => {
                return ParserError::ExpectedType
                    .to_diagnostic()
                    .with_label(token.span.label("expected a type"))
                    .with_hint(format!("{} cannot be parsed as a type", token.kind))
                    .to_err()
            }
        };

        Ok(Type {
            kind,
            span: token.span,
        })
    }
}

use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::StringType, "expected a string type")?;

    Ok(TypeValue::String.with_span(token.span))
}

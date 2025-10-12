use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::UnitType, "expected a unit type")?;

    Ok(TypeValue::Unit.with_span(token.span))
}

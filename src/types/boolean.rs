use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::BooleanType, "expected a boolean type")?;

    Ok(Type::new(token, TypeValue::Boolean))
}

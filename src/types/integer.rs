use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::IntegerType, "expected an integer type")?;

    Ok(Type::new(token, TypeValue::Integer))
}

use crate::prelude::*;

pub fn parse_i32(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::I32Type, "expected an 32 bit integer type")?;

    Ok(Type::new(token, TypeValue::I32))
}

pub fn parse_i64(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::I64Type, "expected an 64 bit integer type")?;

    Ok(Type::new(token, TypeValue::I64))
}

use crate::prelude::*;

/// Parse a boolean type
pub fn parse_boolean(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::BooleanType, "expected a boolean type")?;
    Ok(Type::new(token, TypeValue::Boolean))
}

/// Parse a string type
pub fn parse_string(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::StringType, "expected a string type")?;
    Ok(TypeValue::String.with_span(token.span))
}

/// Parse a unit type
pub fn parse_unit(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::UnitType, "expected a unit type")?;
    Ok(TypeValue::Unit.with_span(token.span))
}

/// Parse a 32-bit integer type
pub fn parse_i32(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::I32Type, "expected an 32 bit integer type")?;
    Ok(Type::new(token, TypeValue::I32))
}

/// Parse a 64-bit integer type
pub fn parse_i64(parser: &mut Parser) -> Result<Type> {
    let token = parser.expect(TokenKind::I64Type, "expected an 64 bit integer type")?;
    Ok(Type::new(token, TypeValue::I64))
}

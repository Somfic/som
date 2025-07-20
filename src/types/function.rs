use crate::prelude::*;

pub fn parse(parser: &mut Parser) -> Result<Type> {
    let fn_token = parser.expect(TokenKind::Function, "expected 'fn' keyword")?;

    // Parse parameter list (i32, i64, bool, etc.)
    parser.expect(TokenKind::ParenOpen, "expected '(' after 'fn'")?;

    let mut parameters = Vec::new();

    // Parse parameters if there are any
    loop {
        // Check if we're at the end of the parameter list
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        // Parse parameter type
        let param_type = parser.parse_type(BindingPower::None)?;

        // Create a dummy parameter with empty identifier for type annotation
        let param = Parameter {
            identifier: Identifier::new("", fn_token.span),
            type_: Box::new(param_type),
            span: fn_token.span,
        };
        parameters.push(param);

        // Check for comma or end of parameter list
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::Comma)
        }) {
            parser.expect(TokenKind::Comma, "expected ','")?; // consume comma
        } else {
            break;
        }
    }

    parser.expect(TokenKind::ParenClose, "expected ')' after parameter list")?;

    // Parse return type
    parser.expect(TokenKind::Arrow, "expected '->' after parameter list")?;
    let return_type = parser.parse_type(BindingPower::None)?;

    let span = fn_token.span + return_type.span;

    let function_type = FunctionType {
        parameters,
        return_type: Box::new(return_type),
        span: span,
    };

    Ok(Type::new(span, TypeValue::Function(function_type)))
}

use crate::parser::{
    ast::{BinaryOperation, Expression},
    lookup::{BindingPower, Lookup},
    macros::expect_token,
    ParseResult, Parser,
};

pub fn register(lookup: &mut Lookup) {
    use crate::scanner::lexeme::TokenType;

    lookup.add_left_expression_handler(TokenType::Plus, BindingPower::Additive, parse_addative);
    lookup.add_left_expression_handler(TokenType::Minus, BindingPower::Additive, parse_subtractive);
    lookup.add_left_expression_handler(
        TokenType::Star,
        BindingPower::Multiplicative,
        parse_multiplicative,
    );
    lookup.add_left_expression_handler(
        TokenType::Slash,
        BindingPower::Multiplicative,
        parse_dividing,
    );
}

fn parse_addative<'a>(
    parser: &mut Parser<'a>,
    left: Expression,
    binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    expect_token!(parser, Plus)?;
    let right = super::parse(parser, binding_power)?;

    Ok(Expression::Binary(
        Box::new(left),
        BinaryOperation::Plus,
        Box::new(right),
    ))
}

fn parse_subtractive<'a>(
    parser: &mut Parser<'a>,
    left: Expression,
    binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    expect_token!(parser, Minus)?;
    let right = super::parse(parser, binding_power)?;

    Ok(Expression::Binary(
        Box::new(left),
        BinaryOperation::Minus,
        Box::new(right),
    ))
}

fn parse_multiplicative<'a>(
    parser: &mut Parser<'a>,
    left: Expression,
    binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    expect_token!(parser, Star)?;
    let right = super::parse(parser, binding_power)?;

    Ok(Expression::Binary(
        Box::new(left),
        BinaryOperation::Times,
        Box::new(right),
    ))
}

fn parse_dividing<'a>(
    parser: &mut Parser<'a>,
    left: Expression,
    binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    expect_token!(parser, Slash)?;
    let right = super::parse(parser, binding_power)?;

    Ok(Expression::Binary(
        Box::new(left),
        BinaryOperation::Divide,
        Box::new(right),
    ))
}

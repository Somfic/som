use std::collections::HashMap;

use crate::{
    parser::{
        ast::Expression,
        lookup::{BindingPower, Lookup},
        macros::{expect_token, expect_value, optional_token},
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    use crate::scanner::lexeme::TokenType;

    lookup.add_left_expression_handler(
        TokenType::ParenOpen,
        BindingPower::Primary,
        parse_function_call,
    );
}

pub fn parse_function_call<'a>(
    parser: &mut crate::parser::Parser<'a>,
    left_hand_side: crate::parser::ast::Expression,
    _binding_power: BindingPower,
) -> crate::parser::ParseResult<'a, crate::parser::ast::Expression> {
    let identifier = match left_hand_side {
        Expression::Identifier(identifier) => identifier.clone(),
        _ => {
            println!("{:?}", left_hand_side);
            unreachable!()
        }
    };

    expect_token!(parser, ParenOpen)?;

    let mut parameters = Vec::new();

    loop {
        let token = parser.peek().unwrap();

        if token.token_type == TokenType::ParenClose {
            break;
        }

        let expression = super::parse(parser, BindingPower::None)?;

        parameters.push(expression);

        optional_token!(parser, Comma);
    }

    expect_token!(parser, ParenClose)?;

    Ok(Expression::FunctionCall(identifier, parameters))
}

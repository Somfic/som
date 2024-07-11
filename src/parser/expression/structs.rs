use std::collections::HashMap;

use crate::{
    parser::{
        ast::Expression,
        lookup::BindingPower,
        macros::{expect_token, expect_value, optional_token},
        Lookup, ParseResult,
    },
    scanner::lexeme::TokenType,
};

pub fn register(lookup: &mut Lookup) {
    lookup.add_left_expression_handler(
        TokenType::CurlyOpen,
        BindingPower::Primary,
        parse_struct_initializer,
    );
}

fn parse_struct_initializer<'a>(
    parser: &mut crate::parser::Parser<'a>,
    left_hand_side: crate::parser::ast::Expression,
    _binding_power: BindingPower,
) -> ParseResult<'a, Expression> {
    let identifier = match left_hand_side {
        Expression::Identifier(identifier) => identifier.clone(),
        _ => {
            unreachable!()
        }
    };

    expect_token!(parser, CurlyOpen)?;

    let mut fields = HashMap::new();

    loop {
        let token = parser.peek().unwrap();

        if token.token_type == TokenType::CurlyClose {
            break;
        }

        let identifier = expect_token!(parser, Identifier)?;
        let identifier = expect_value!(identifier, Identifier);

        expect_token!(parser, Colon)?;

        let expression = super::parse(parser, BindingPower::None)?;

        fields.insert(identifier, expression);

        optional_token!(parser, Comma);
    }

    expect_token!(parser, CurlyClose)?;

    Ok(Expression::StructInitializer(identifier, fields))
}

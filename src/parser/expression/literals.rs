use crate::parser::{
    ast::Expression,
    lookup::Lookup,
    macros::{expect_token, expect_value},
    ParseResult, Parser,
};

pub(crate) fn register(lookup: &mut Lookup) {
    use crate::scanner::lexeme::TokenType;

    lookup
        .add_expression_handler(TokenType::Decimal, parse_decimal)
        .add_expression_handler(TokenType::Integer, parse_integer)
        .add_expression_handler(TokenType::String, parse_string)
        .add_expression_handler(TokenType::Identifier, parse_identifier)
        .add_expression_handler(TokenType::Boolean, parse_boolean);
}

fn parse_decimal<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let decimal = expect_token!(parser, Decimal)?;
    let decimal = *expect_value!(decimal, Decimal);

    Ok(Expression::Number(decimal))
}

fn parse_integer<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let integer = expect_token!(parser, Integer)?;
    let integer = *expect_value!(integer, Integer);

    Ok(Expression::Number(integer as f64))
}

fn parse_string<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let string = expect_token!(parser, String)?;
    let string = expect_value!(string, String).clone();

    Ok(Expression::String(string))
}

fn parse_identifier<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let identifier = expect_token!(parser, Identifier)?;
    let identifier = expect_value!(identifier, Identifier).clone();

    Ok(Expression::Identifier(identifier))
}

fn parse_boolean<'a>(parser: &mut Parser<'a>) -> ParseResult<'a, Expression> {
    let boolean = expect_token!(parser, Boolean)?;
    let boolean = *expect_value!(boolean, Boolean);

    Ok(Expression::Boolean(boolean))
}

use miette::diagnostic;

use super::{lookup::BindingPower, Parser};
use crate::ast::{
    BinaryOperator, Expression, ExpressionValue, Primitive, Spannable, StatementValue,
    UnaryOperator,
};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, TokenValue};

pub fn parse_integer<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Integer, "expected an integer")?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Integer(value)),
    ))
}

pub fn parse_decimal<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Decimal(value)),
    ))
}

pub fn parse_string<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::String, "expected a string")?;

    let value = match token.value {
        TokenValue::String(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::String(value)),
    ))
}

fn parse_binary_expression<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
    operator: BinaryOperator,
) -> ParserResult<Expression<'ast>> {
    let rhs = parser.parse_expression(bp)?;

    Ok(Expression::at_multiple(
        vec![rhs.span, lhs.span],
        ExpressionValue::Binary {
            operator,
            left: Box::new(lhs),
            right: Box::new(rhs),
        },
    ))
}

pub fn parse_binary_plus<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Add)
}

pub fn parse_binary_subtract<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Subtract)
}

pub fn parse_binary_multiply<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Multiply)
}

pub fn parse_binary_divide<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Divide)
}

pub fn parse_binary_less_than<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThan)
}

pub fn parse_binary_greater_than<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThan)
}

pub fn parse_binary_less_than_or_equal<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThanOrEqual)
}

pub fn parse_binary_greater_than_or_equal<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThanOrEqual)
}

pub fn parse_binary_equal<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Equality)
}

pub fn parse_binary_not_equal<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Inequality)
}

pub fn parse_binary_and<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::And)
}

pub fn parse_binary_or<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Or)
}

pub fn parse_group<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::ParenOpen, "expected the start of the grouping")?;

    let expression = parser.parse_expression(BindingPower::None)?;

    parser
        .tokens
        .expect(TokenKind::ParenClose, "expected the end of the grouping")?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Group(Box::new(expression)),
    ))
}

pub fn parse_unary_negation<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Not, "expected a negation sign")?;

    let expression = parser.parse_expression(BindingPower::Unary)?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Unary {
            operator: UnaryOperator::Negate,
            operand: Box::new(expression),
        },
    ))
}

pub fn parse_unary_negative<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Minus, "expected a negative sign")?;

    let expression = parser.parse_expression(BindingPower::Unary)?;

    Ok(Expression::at_multiple(
        vec![token.span, expression.span],
        ExpressionValue::Unary {
            operator: UnaryOperator::Negative,
            operand: Box::new(expression),
        },
    ))
}

pub fn parse_boolean<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Boolean(value)),
    ))
}

pub fn parse_conditional<'ast>(
    parser: &mut Parser<'ast>,
    truthy: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    let condition = parser.parse_expression(BindingPower::None)?;

    parser.tokens.expect(TokenKind::Else, "expected an else")?;

    let falsy = parser.parse_expression(bp)?;

    if true {
        // truthy
    } else {
        // falsy
    }

    Ok(Expression::at_multiple(
        vec![condition.span, truthy.span, falsy.span],
        ExpressionValue::Conditional {
            condition: Box::new(condition),
            truthy: Box::new(truthy),
            falsy: Box::new(falsy),
        },
    ))
}
pub fn parse_inner_block<'ast>(
    parser: &mut Parser<'ast>,
    terminating_token: TokenKind,
) -> ParserResult<Expression<'ast>> {
    let mut statements = Vec::new();
    let mut result_expr = None;

    while let Some(token) = parser.tokens.peek() {
        if token.as_ref().is_ok_and(|t| t.kind == terminating_token) {
            break;
        }

        let statement = parser.parse_statement(false)?;

        if let Some(next_token) = parser.tokens.peek() {
            if next_token
                .as_ref()
                .is_ok_and(|t| t.kind == TokenKind::Semicolon)
            {
                parser
                    .tokens
                    .expect(TokenKind::Semicolon, "expected a semicolon")?;
                statements.push(statement);
                continue;
            }
        }

        match statement.value {
            StatementValue::Expression(expr) => {
                result_expr = Some(expr);
            }
            _ => {
                todo!("block statements can only be expressions");
            }
        }
        break;
    }

    let spans = statements.iter().map(|s| s.span).collect();

    match result_expr {
        Some(expr) => Ok(Expression::at_multiple(
            spans,
            ExpressionValue::Block {
                statements,
                result: Box::new(expr),
            },
        )),
        None => Ok(Expression::at_multiple(
            spans,
            ExpressionValue::Primitive(Primitive::Unit),
        )),
    }
}

pub fn parse_block<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    parser.tokens.expect(
        TokenKind::CurlyOpen,
        "expected the start of an expression block",
    )?;

    let inner_block = parse_inner_block(parser, TokenKind::CurlyClose)?;

    parser.tokens.expect(
        TokenKind::CurlyClose,
        "expected the end of the expression block",
    )?;

    Ok(inner_block)
}

pub fn parse_identifier<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let name = match token.value {
        TokenValue::Identifier(name) => name,
        _ => unreachable!(),
    };

    Ok(Expression::at(
        token.span,
        ExpressionValue::Primitive(Primitive::Identifier(name)),
    ))
}

pub fn parse_function_call<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    let function_name = match lhs.value {
        ExpressionValue::Primitive(Primitive::Identifier(name)) => name,
        _ => todo!("function calls on non-identifiers"),
    };

    let mut arguments = Vec::new();

    loop {
        if parser.tokens.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !arguments.is_empty() {
            parser.tokens.expect(TokenKind::Comma, "expected a comma")?;
        }

        let argument = parser.parse_expression(BindingPower::None)?;
        arguments.push(argument);
    }

    let close = parser
        .tokens
        .expect(TokenKind::ParenClose, "expected the end of a function call")?;

    Ok(Expression::at_multiple(
        vec![lhs.span, close.span],
        ExpressionValue::FunctionCall {
            function_name,
            arguments,
        },
    ))
}

pub fn parse_assignment<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    let name = match lhs.value {
        ExpressionValue::Primitive(Primitive::Identifier(name)) => Ok(name),
        _ => Err(vec![diagnostic!(
            labels = vec![lhs.label("expected a variable name")],
            help = "assignments can only be made to variables",
            "invalid assign target"
        )]),
    }?;

    let value = parser.parse_expression(bp)?;

    Ok(Expression::at_multiple(
        vec![lhs.span, value.span],
        ExpressionValue::Assignment {
            name,
            value: Box::new(value),
        },
    ))
}

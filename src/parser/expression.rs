use miette::diagnostic;

use super::{lookup::BindingPower, Parser};
use crate::ast::{
    combine_spans, BinaryOperator, CombineSpan, Expression, ExpressionValue, Primitive, Spannable,
    StatementValue, UnaryOperator,
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

    Ok(ExpressionValue::Primitive(Primitive::Integer(value)).with_span(token.span))
}

pub fn parse_decimal<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Decimal(value)).with_span(token.span))
}

pub fn parse_string<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::String, "expected a string")?;

    let value = match token.value {
        TokenValue::String(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::String(value)).with_span(token.span))
}

fn parse_binary_expression<'ast>(
    parser: &mut Parser<'ast>,
    lhs: Expression<'ast>,
    bp: BindingPower,
    operator: BinaryOperator,
) -> ParserResult<Expression<'ast>> {
    let rhs = parser.parse_expression(bp)?;
    let span = lhs.span.combine(rhs.span);

    Ok(ExpressionValue::Binary {
        operator,
        left: Box::new(lhs),
        right: Box::new(rhs),
    }
    .with_span(span))
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

    let span = token.span.combine(expression.span);

    Ok(ExpressionValue::Group(Box::new(expression)).with_span(span))
}

pub fn parse_unary_negation<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Not, "expected a negation sign")?;

    let expression = parser.parse_expression(BindingPower::Unary)?;

    let span = token.span.combine(expression.span);

    Ok(ExpressionValue::Unary {
        operator: UnaryOperator::Negate,
        operand: Box::new(expression),
    }
    .with_span(span))
}

pub fn parse_unary_negative<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Minus, "expected a negative sign")?;

    let expression = parser.parse_expression(BindingPower::Unary)?;

    let span = token.span.combine(expression.span);

    Ok(ExpressionValue::Unary {
        operator: UnaryOperator::Negative,
        operand: Box::new(expression),
    }
    .with_span(span))
}

pub fn parse_boolean<'ast>(parser: &mut Parser<'ast>) -> ParserResult<Expression<'ast>> {
    let token = parser
        .tokens
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Boolean(value)).with_span(token.span))
}

pub fn parse_conditional<'ast>(
    parser: &mut Parser<'ast>,
    truthy: Expression<'ast>,
    bp: BindingPower,
) -> ParserResult<Expression<'ast>> {
    let condition = parser.parse_expression(BindingPower::None)?;

    parser.tokens.expect(TokenKind::Else, "expected an else")?;

    let falsy = parser.parse_expression(bp)?;

    let span = condition.span.combine(truthy.span).combine(falsy.span);

    Ok(ExpressionValue::Conditional {
        condition: Box::new(condition),
        truthy: Box::new(truthy),
        falsy: Box::new(falsy),
    }
    .with_span(span))
}

pub fn parse_inner_block<'ast>(
    parser: &mut Parser<'ast>,
    terminating_token: TokenKind,
) -> ParserResult<Expression<'ast>> {
    let mut statements = Vec::new();
    let mut final_expression = None;

    while let Some(token) = parser.tokens.peek() {
        if token.as_ref().is_ok_and(|t| t.kind == terminating_token) {
            break;
        }

        let statement = parser.parse_statement(false)?;

        // Check if the next token is a semicolon
        let is_semicolon = parser.tokens.peek().as_ref().is_some_and(|t| {
            t.as_ref()
                .ok()
                .map(|t| t.kind == TokenKind::Semicolon)
                .unwrap_or(false)
        });

        if is_semicolon {
            // Consume the semicolon and treat this as a statement
            parser
                .tokens
                .expect(TokenKind::Semicolon, "expected a semicolon")?;
            statements.push(statement);
        } else {
            // If no semicolon, validate that this is an Expression
            if final_expression.is_some() {
                return Err(vec![diagnostic! {
                    labels = vec![statement.label("missing semicolon before this statement")],
                    help = "Add a semicolon to separate the statements.",
                    "expected a semicolon before the next statement"
                }]);
            }

            match &statement.value {
                StatementValue::Expression(expression) => {
                    final_expression = Some(expression.clone());
                }
                _ => {
                    return Err(vec![diagnostic! {
                        labels = vec![statement.label("this statement must end with a semicolon")],
                        help = "Only expressions can be used as the final statement in a block.",
                        "expected a semicolon"
                    }]);
                }
            }

            parser
                .tokens
                .expect(terminating_token, "expected the end of the block")?;
            break;
        }
    }

    let span = combine_spans(statements.iter().map(|s| s.span).collect());

    let final_expression = match final_expression {
        Some(expression) => expression,
        None => ExpressionValue::Primitive(Primitive::Unit).with_span(span),
    };

    Ok(ExpressionValue::Block {
        statements,
        result: Box::new(final_expression),
    }
    .with_span(span))
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

    Ok(ExpressionValue::Primitive(Primitive::Identifier(name)).with_span(token.span))
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

    let span = lhs.span.combine(close.span);

    Ok(ExpressionValue::FunctionCall {
        function_name,
        arguments,
    }
    .with_span(span))
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

    let span = lhs.span.combine(value.span);

    Ok(ExpressionValue::Assignment {
        name,
        value: Box::new(value),
    }
    .with_span(span))
}

use std::collections::HashMap;

use miette::diagnostic;

use super::{lookup::BindingPower, Parser};
use crate::ast::{
    combine_spans, BinaryOperator, CombineSpan, Expression, ExpressionValue, Identifier, Primitive,
    StatementValue, UnaryOperator,
};
use crate::prelude::*;
use crate::tokenizer::{TokenKind, TokenValue};

pub fn parse_integer(parser: &mut Parser) -> Result<Expression> {
    let token = parser
        .tokens
        .expect(TokenKind::Integer, "expected an integer")?;

    let value = match token.value {
        TokenValue::Integer(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Integer(value)).with_span(token.span))
}

pub fn parse_decimal(parser: &mut Parser) -> Result<Expression> {
    let token = parser
        .tokens
        .expect(TokenKind::Decimal, "expected a decimal")?;

    let value = match token.value {
        TokenValue::Decimal(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Decimal(value)).with_span(token.span))
}

pub fn parse_string(parser: &mut Parser) -> Result<Expression> {
    let token = parser
        .tokens
        .expect(TokenKind::String, "expected a string")?;

    let value = match token.value {
        TokenValue::String(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::String(value)).with_span(token.span))
}

fn parse_binary_expression(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
    operator: BinaryOperator,
) -> Result<Expression> {
    let rhs = parser.parse_expression(bp)?;
    let span = lhs.span.combine(rhs.span);

    Ok(ExpressionValue::Binary {
        operator,
        left: Box::new(lhs),
        right: Box::new(rhs),
    }
    .with_span(span))
}

pub fn parse_binary_plus(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Add)
}

pub fn parse_binary_subtract(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Subtract)
}

pub fn parse_binary_multiply(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Multiply)
}

pub fn parse_binary_divide(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Divide)
}

pub fn parse_binary_less_than(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThan)
}

pub fn parse_binary_greater_than(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThan)
}

pub fn parse_binary_less_than_or_equal(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::LessThanOrEqual)
}

pub fn parse_binary_greater_than_or_equal(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::GreaterThanOrEqual)
}

pub fn parse_binary_equal(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Equality)
}

pub fn parse_binary_not_equal(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Inequality)
}

pub fn parse_binary_and(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::And)
}

pub fn parse_binary_or(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    parse_binary_expression(parser, lhs, bp, BinaryOperator::Or)
}

pub fn parse_group(parser: &mut Parser) -> Result<Expression> {
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

pub fn parse_unary_negation(parser: &mut Parser) -> Result<Expression> {
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

pub fn parse_unary_negative(parser: &mut Parser) -> Result<Expression> {
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

pub fn parse_boolean(parser: &mut Parser) -> Result<Expression> {
    let token = parser
        .tokens
        .expect(TokenKind::Boolean, "expected a boolean")?;

    let value = match token.value {
        TokenValue::Boolean(value) => value,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Boolean(value)).with_span(token.span))
}

pub fn parse_conditional(
    parser: &mut Parser,
    truthy: Expression,
    bp: BindingPower,
) -> Result<Expression> {
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

pub fn parse_inner_block(parser: &mut Parser, terminating_token: TokenKind) -> Result<Expression> {
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

pub fn parse_block(parser: &mut Parser) -> Result<Expression> {
    parser.tokens.expect(
        TokenKind::CurlyOpen,
        "expected the start of an expression block",
    )?;

    let inner_block = parse_inner_block(parser, TokenKind::CurlyClose)?;

    Ok(inner_block)
}

pub fn parse_identifier(parser: &mut Parser) -> Result<Expression> {
    let token = parser
        .tokens
        .expect(TokenKind::Identifier, "expected an identifier")?;

    let name = match token.value {
        TokenValue::Identifier(name) => name,
        _ => unreachable!(),
    };

    Ok(ExpressionValue::Primitive(Primitive::Identifier(name)).with_span(token.span))
}

pub fn parse_function_call(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
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
        function: Box::new(lhs),
        arguments,
    }
    .with_span(span))
}

pub fn parse_assignment(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    let identifier = Identifier::from_expression(&lhs)?;

    let value = parser.parse_expression(bp)?;

    let span = lhs.span.combine(value.span);

    Ok(ExpressionValue::VariableAssignment {
        identifier,
        argument: Box::new(value),
    }
    .with_span(span))
}

pub fn parse_field_access(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    let parent_identifier = Identifier::from_expression(&lhs)?;

    let value = parser.parse_expression(bp)?;

    let identifier = Identifier::from_expression(&value)?;

    let span = lhs.span.combine(value.span);

    Ok(ExpressionValue::FieldAccess {
        parent_identifier,
        identifier,
    }
    .with_span(span))
}

pub fn parse_struct_constructor(
    parser: &mut Parser,
    lhs: Expression,
    bp: BindingPower,
) -> Result<Expression> {
    println!("lhs: {lhs:?}");

    let identifier = Identifier::from_expression(&lhs)?;

    let mut arguments = HashMap::new();

    while let Some(token) = parser.tokens.peek() {
        if token
            .as_ref()
            .is_ok_and(|token| token.kind == TokenKind::CurlyClose)
        {
            break;
        }

        if !arguments.is_empty() {
            parser.tokens.expect(TokenKind::Comma, "expected a comma")?;
        }

        let identifier = parser
            .tokens
            .expect(TokenKind::Identifier, "expected a field name")?;

        let identifier = Identifier::from_token(&identifier)?;

        parser
            .tokens
            .expect(TokenKind::Equal, "expected a field value")?;

        let value = parser.parse_expression(BindingPower::None)?;

        // TODO: error if the field already defined
        arguments.insert(identifier, value);
    }

    let close = parser
        .tokens
        .expect(TokenKind::CurlyClose, "expected the end of the fields")?;

    let span = combine_spans(
        arguments
            .iter()
            .map(|a| a.0.span.combine(a.1.span))
            .collect(),
    );

    Ok(ExpressionValue::StructConstructor {
        identifier,
        arguments,
    }
    .with_span(span))
}

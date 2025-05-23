use crate::prelude::*;
use std::{collections::HashSet, hash::Hash};

#[derive(Debug, Clone)]
pub struct Argument<Expression> {
    pub identifier: Identifier,
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub struct FunctionExpression<Expression> {
    pub parameters: Vec<Parameter>,
    pub explicit_return_type: Option<Type>,
    pub body: Box<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Parameter {
    pub identifier: Identifier,
    pub type_: Box<Type>,
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Function, "expected a function signature")?;

    parser.expect(TokenKind::ParenOpen, "expected function arguments")?;
    let mut parameters = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !parameters.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Tilde,
            format!("expected a parameter type for `{}`", identifier.name),
        )?;

        let type_ = parser.parse_type(BindingPower::None)?;

        parameters.push(Parameter {
            identifier,
            type_: Box::new(type_),
        });
    }

    parser.expect(TokenKind::ParenClose, "expected function arguments")?;

    let body = parser.parse_expression(BindingPower::None)?;

    let span = token.span + body.span;

    Ok(ExpressionValue::Function(FunctionExpression {
        parameters,
        explicit_return_type: None,
        body: Box::new(body),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut Environment,
) -> TypedExpression {
    let value = match &expression.value {
        ExpressionValue::Function(value) => value,
        _ => unreachable!(),
    };

    let env = &mut env.block();

    for parameter in &value.parameters {
        env.set(&parameter.identifier, &parameter.type_);
    }

    let body = type_checker.check_expression(&value.body, env);

    let type_ = TypeValue::Function {
        parameters: value.parameters.clone(),
        returns: Box::new(body.type_.value.clone()),
    };
    let value = TypedExpressionValue::Function(FunctionExpression {
        parameters: value.parameters.clone(),
        body: Box::new(body),
        explicit_return_type: value.explicit_return_type.clone(),
    });

    expression.with_value_type(value, Type::new(expression, type_))
}

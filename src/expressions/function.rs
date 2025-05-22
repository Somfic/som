use crate::prelude::*;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct Argument<Expression> {
    pub identifier: Identifier,
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub struct FunctionExpression<Expression> {
    pub arguments: Vec<Argument<Expression>>,
    pub explicit_return_type: Option<Type>,
    pub body: Box<Expression>,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub identifier: Identifier,
    pub type_: Type,
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let token = parser.expect(TokenKind::Function, "expected a function signature")?;

    parser.expect(TokenKind::ParenOpen, "expected function arguments")?;
    let mut arguments = vec![];

    loop {
        if parser.peek().is_some_and(|token| {
            token
                .as_ref()
                .is_ok_and(|token| token.kind == TokenKind::ParenClose)
        }) {
            break;
        }

        if !arguments.is_empty() {
            parser.expect(TokenKind::Comma, "expected a comma between arguments")?;
        }

        let identifier = parser.expect_identifier()?;

        parser.expect(
            TokenKind::Tilde,
            format!("expected a parameter type for `{}`", identifier.name),
        )?;

        let value = parser.parse_expression(BindingPower::None)?;

        arguments.push(Argument { identifier, value });
    }

    parser.expect(TokenKind::ParenClose, "expected function arguments")?;

    parser.expect(TokenKind::Arrow, "expected a function body")?;

    let body = parser.parse_expression(BindingPower::None)?;

    let span = token.span + body.span;

    Ok(ExpressionValue::Function(FunctionExpression {
        arguments,
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

    let typed_arguments = vec![];
    for argument in &value.arguments {
        let value = type_checker.check_expression(&argument.value, env);
        typed_arguments.push(Argument {
            identifier: argument.identifier.clone(),
            value,
        });
        env.set(&argument.identifier, &value.type_);
    }

    let env = &mut env.block();

    let body = type_checker.check_expression(&value.body, env);

    let value = TypedExpressionValue::Function(FunctionExpression {
        arguments: typed_arguments,
        body: Box::new(body),
        explicit_return_type: value.explicit_return_type.clone(),
    });

    expression.with_value_type(value, Type::new(expression, TypeValue::Unit))
}

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct BlockExpression<Expression> {
    pub statements: Vec<GenericStatement<Expression>>,
    pub result: Box<Expression>,
}

pub fn parse(parser: &mut Parser) -> Result<Expression> {
    let start = parser.expect(TokenKind::CurlyOpen, "expected a block")?;

    let inner = parse_inner(parser, TokenKind::CurlyClose)?;

    let end = parser.expect(TokenKind::CurlyClose, "expected the end of the block")?;

    Ok(inner.value.with_span(start.span + end.span))
}

pub fn parse_inner(parser: &mut Parser, terminating_token: TokenKind) -> Result<Expression> {
    let mut statements = Vec::new();
    let mut final_expression = None;

    while let Some(token) = parser.peek() {
        if token.as_ref().is_ok_and(|t| t.kind == terminating_token) {
            break;
        }

        let statement = parser.parse_statement(false)?;

        // Check if the next token is a semicolon
        let is_semicolon = parser.peek().as_ref().is_some_and(|t| {
            t.as_ref()
                .ok()
                .map(|t| t.kind == TokenKind::Semicolon)
                .unwrap_or(false)
        });

        if is_semicolon {
            parser.expect(TokenKind::Semicolon, "expected a semicolon")?;
            statements.push(statement);
        } else {
            if final_expression.is_some() {
                return Err(parser_expected_semicolon(
                    parser.current().unwrap().as_ref().unwrap(),
                ));
            }

            match &statement.value {
                StatementValue::Expression(expression) => {
                    final_expression = Some(expression.clone());
                }
                _ => {
                    return Err(parser_expected_semicolon(
                        parser.current().unwrap().as_ref().unwrap(),
                    ));
                }
            }

            break;
        }
    }

    if statements.is_empty() && final_expression.is_none() {
        return Err(parser_unexpected_end_of_file(
            (parser.lexer.byte_offset, 0),
            "a statement or an expression",
        ));
    }

    let spans = statements
        .iter()
        .map(|statement| statement.span)
        .chain(final_expression.as_ref().map(|e| e.span))
        .collect::<Vec<_>>();

    let start = *spans.first().unwrap();
    let end = *spans.last().unwrap();
    let span = start + end;

    let final_expression = match final_expression {
        Some(expression) => expression,
        None => ExpressionValue::Primary(PrimaryExpression::Unit).with_span(span),
    };

    Ok(ExpressionValue::Block(BlockExpression {
        statements,
        result: Box::new(final_expression),
    })
    .with_span(span))
}

pub fn type_check(
    type_checker: &mut TypeChecker,
    expression: &Expression,
    env: &mut TypeEnvironment,
) -> TypedExpression {
    let block = match &expression.value {
        ExpressionValue::Block(block) => block,
        _ => unreachable!(),
    };

    let mut env = env.block();

    let mut statements = Vec::new();

    for statement in &block.statements {
        statements.push(type_checker.check_statement(statement, &mut env));
    }

    let result = type_checker.check_expression(&block.result, &mut env);

    let type_ = Type::new(result.span, result.type_.value.clone());
    let value = TypedExpressionValue::Block(BlockExpression {
        statements,
        result: Box::new(result),
    });

    expression.with_value_type(value, type_)
}

pub fn compile(
    compiler: &mut Compiler,
    block: &BlockExpression<TypedExpression>,
    body: &mut FunctionBuilder,
    env: &mut CompileEnvironment,
) -> CompileValue {
    let mut env = env.block();

    for statement in &block.statements {
        compiler.compile_statement(statement, body, &mut env);
    }

    compiler.compile_expression(&block.result, body, &mut env)
}

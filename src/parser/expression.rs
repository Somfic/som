use crate::{
    ast::{Binary, BinaryOperation, Block, Expr, Expression, Group, Primary, Statement, Unary},
    lexer::{Token, TokenKind, TokenValue},
    parser::{Parse, Untyped},
    Parser, ParserError, Result,
};

impl Parse for Expression<Untyped> {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let (expr, span) = input.parse_with_span_with::<Expr<Untyped>>(min_bp)?;

        Ok(Expression { expr, span, ty: () })
    }
}

impl Parse for Binary<Untyped> {
    type Params = Expression<Untyped>;

    fn parse(input: &mut Parser, lhs: Self::Params) -> Result<Self> {
        let op = input.next()?;

        let Some(&(_, r_bp)) = input.lookup.binding_power_lookup.get(&op.kind) else {
            return ParserError::InvalidBinaryOperator
                .to_diagnostic()
                .with_label(op.span.label("expected a binary operator"))
                .to_err();
        };

        let rhs = input.parse_with(r_bp)?;

        let operation = match op.kind {
            TokenKind::Plus => BinaryOperation::Add,
            TokenKind::Minus => BinaryOperation::Subtract,
            TokenKind::Star => BinaryOperation::Multiply,
            TokenKind::Slash => BinaryOperation::Divide,
            _ => {
                return ParserError::InvalidBinaryOperator
                    .to_diagnostic()
                    .with_label(op.span.label("expected a binary operator"))
                    .with_hint(format!(
                    "{} cannot be used as a binary operator. only {}, {}, {} and {} are supported",
                    op.kind,
                    TokenKind::Plus,
                    TokenKind::Minus,
                    TokenKind::Star,
                    TokenKind::Slash
                ))
                    .to_err()
            }
        };

        Ok(Binary {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            op: operation,
        })
    }
}

impl Parse for Expr<Untyped> {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let peek = input.peek_expect("an expression")?.clone();

        let Some(parse_function) = input.lookup.expression_lookup.get(&peek.kind).cloned() else {
            return ParserError::ExpectedExpression
                .to_diagnostic()
                .with_label(peek.span.clone().label("expected this to be an expression"))
                .with_hint(format!("{} cannot be parsed as an expression", peek))
                .to_err();
        };

        let mut lhs = parse_function(input)?;

        loop {
            let Some(token) = input.peek() else { break };
            let token_kind = token.kind.clone();

            let Some(lefthand_parse_function) = input
                .lookup
                .lefthand_expression_lookup
                .get(&token_kind)
                .cloned()
            else {
                break;
            };

            let Some(&(l_bp, _)) = input.lookup.binding_power_lookup.get(&token_kind) else {
                break;
            };

            if l_bp < min_bp {
                break;
            }

            lhs = lefthand_parse_function(input, lhs)?;
        }

        Ok(lhs.expr)
    }
}

impl Parse for Primary {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        match input.next()? {
            Token {
                kind: TokenKind::Boolean,
                value: TokenValue::Boolean(b),
                ..
            } => Ok(Primary::Boolean(b)),
            Token {
                kind: TokenKind::I32,
                value: TokenValue::I32(i),
                ..
            } => Ok(Primary::I32(i)),
            Token {
                kind: TokenKind::I64,
                value: TokenValue::I64(i),
                ..
            } => Ok(Primary::I64(i)),
            Token {
                kind: TokenKind::Decimal,
                value: TokenValue::Decimal(d),
                ..
            } => Ok(Primary::Decimal(d)),
            Token {
                kind: TokenKind::String,
                value: TokenValue::String(s),
                ..
            } => Ok(Primary::String(s)),
            Token {
                kind: TokenKind::Character,
                value: TokenValue::Character(c),
                ..
            } => Ok(Primary::Character(c)),
            Token {
                kind: TokenKind::Identifier,
                value: TokenValue::Identifier(id),
                ..
            } => Ok(Primary::Identifier(id)),
            token => ParserError::InvalidPrimaryExpression
                .to_diagnostic()
                .with_label(token.span.label("expected a primary"))
                .with_hint(format!("{} cannot be parsed as a primary", token.kind))
                .to_err(),
        }
    }
}

impl Parse for Unary<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.next()?;

        match token.kind {
            TokenKind::Minus => {
                todo!()
            }
            _ => ParserError::InvalidUnaryExpression
                .to_diagnostic()
                .with_label(token.span.label("expected this to be a unary operator"))
                .to_err(),
        }
    }
}

impl Parse for Group<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        input.expect(
            TokenKind::ParenOpen,
            "'(' to start a grouped expression",
            ParserError::ExpectedGroupStart,
        )?;

        let expr = input.parse_with::<Expression<Untyped>>(0)?;

        input.expect(
            TokenKind::ParenClose,
            "')' to end a grouped expression",
            ParserError::ExpectedGroupEnd,
        )?;

        Ok(Group {
            expr: Box::new(expr),
        })
    }
}

impl Parse for Block<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        input.expect(
            TokenKind::CurlyOpen,
            "start of block",
            ParserError::ExpectedBlockStart,
        )?;

        let mut statements = vec![];
        let mut expression = None;

        while let Some(token) = input.peek() {
            if token.kind == TokenKind::CurlyClose {
                break;
            }

            // parse statement without parsing the semicolon

            let statement = input.parse_with::<Statement<_>>(false)?;

            // if the statement had a closing semicolon, consume it and try to parse the next statement
            if let Some(Token {
                kind: TokenKind::Semicolon,
                ..
            }) = input.peek()
            {
                input.next()?;
                statements.push(statement);
                continue;
            }

            // there was no semicolon, this is the returning expression
            match statement {
                Statement::Expression(e) => expression = Some(e),
                s => ParserError::InvalidReturningExpression
                    .to_diagnostic()
                    .with_hint(format!("{} cannot be used as a value", s))
                    .to_err()?,
            }
        }

        input.expect(
            TokenKind::CurlyClose,
            "end of block",
            ParserError::ExpectedBlockEnd,
        )?;

        Ok(Block {
            statements,
            expression: expression.map(Box::new),
        })
    }
}

use crate::{
    ast::{
        Binary, BinaryOperation, Block, Call, Expression, Group, Lambda, Parameter, Primary,
        PrimaryKind, Statement, Ternary, Type, Unary,
    },
    lexer::{Identifier, Token, TokenKind, TokenValue},
    parser::{lookup::Precedence, Parse, Untyped},
    Parser, ParserError, Result,
};

impl Parse for Expression<Untyped> {
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

        Ok(lhs)
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

        let rhs = input.parse_with::<Expression<_>>(r_bp)?;

        let operation = match op.kind {
            TokenKind::Plus => BinaryOperation::Add,
            TokenKind::Minus => BinaryOperation::Subtract,
            TokenKind::Star => BinaryOperation::Multiply,
            TokenKind::Slash => BinaryOperation::Divide,
            TokenKind::LessThan => BinaryOperation::LessThan,
            TokenKind::LessThanOrEqual => BinaryOperation::LessThanOrEqual,
            TokenKind::GreaterThan => BinaryOperation::GreaterThan,
            TokenKind::GreaterThanOrEqual => BinaryOperation::GreaterThanOrEqual,
            TokenKind::Equality => BinaryOperation::Equality,
            TokenKind::Inequality => BinaryOperation::Inequality,
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
            span: lhs.span() + rhs.span(),
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            op: operation,
            ty: (),
        })
    }
}

impl Parse for Primary<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.next()?;

        let kind = match &token {
            Token {
                kind: TokenKind::Boolean,
                value: TokenValue::Boolean(b),
                ..
            } => PrimaryKind::Boolean(*b),
            Token {
                kind: TokenKind::I32,
                value: TokenValue::I32(i),
                ..
            } => PrimaryKind::I32(*i),
            Token {
                kind: TokenKind::I64,
                value: TokenValue::I64(i),
                ..
            } => PrimaryKind::I64(*i),
            Token {
                kind: TokenKind::Decimal,
                value: TokenValue::Decimal(d),
                ..
            } => PrimaryKind::Decimal(*d),
            Token {
                kind: TokenKind::String,
                value: TokenValue::String(s),
                ..
            } => PrimaryKind::String(s.clone()),
            Token {
                kind: TokenKind::Character,
                value: TokenValue::Character(c),
                ..
            } => PrimaryKind::Character(*c),
            Token {
                kind: TokenKind::Identifier,
                value: TokenValue::Identifier(ident),
                ..
            } => PrimaryKind::Identifier(ident.clone()),
            token => ParserError::InvalidPrimaryExpression
                .to_diagnostic()
                .with_label(token.span.label("expected a primary"))
                .with_hint(format!("{} cannot be parsed as a primary", token.kind))
                .to_err()?,
        };

        Ok(Primary {
            kind,
            span: token.span,
            ty: (),
        })
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
        let open = input.expect(
            TokenKind::ParenOpen,
            "'(' to start a grouped expression",
            ParserError::ExpectedGroupStart,
        )?;

        let expr = input.parse_with::<Expression<Untyped>>(0)?;

        let close = input.expect(
            TokenKind::ParenClose,
            "')' to end a grouped expression",
            ParserError::ExpectedGroupEnd,
        )?;

        Ok(Group {
            expr: Box::new(expr),
            span: open.span + close.span,
            ty: (),
        })
    }
}

impl Parse for Block<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
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
                    .with_label(s.span().label("this statement"))
                    .with_hint(format!("{} cannot be used as a value", s))
                    .to_err()?,
            }
        }

        let close = input.expect(
            TokenKind::CurlyClose,
            "end of block",
            ParserError::ExpectedBlockEnd,
        )?;

        Ok(Block {
            statements,
            expression: expression.map(Box::new),
            span: open.span + close.span,
            ty: (),
        })
    }
}

impl Parse for Ternary<Untyped> {
    type Params = Expression<Untyped>;

    fn parse(input: &mut Parser, truthy: Self::Params) -> Result<Self> {
        input.expect(TokenKind::If, "a condition", ParserError::ExpectedCondition)?;

        let condition = input.parse()?;

        input.expect(
            TokenKind::Else,
            "a falsy branch",
            ParserError::ExpectedElseBranch,
        )?;

        let falsy = input.parse::<Expression<_>>()?;

        Ok(Ternary {
            span: truthy.span() + falsy.span(),
            condition: Box::new(condition),
            truthy: Box::new(truthy),
            falsy: Box::new(falsy),
            ty: (),
        })
    }
}

impl Parse for Lambda<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let fn_token = input.expect(
            TokenKind::Function,
            "a function",
            ParserError::ExpectedFunction,
        )?;

        input.expect(
            TokenKind::ParenOpen,
            "a list of parameters",
            ParserError::ExpectedParameterList,
        )?;

        let mut parameters = vec![];
        while let Some(token) = input.peek() {
            if token.kind == TokenKind::ParenClose {
                break;
            }

            let name = input.parse::<Identifier>()?;
            input.expect(
                TokenKind::Tilde,
                "'~' before parameter type",
                ParserError::ExpectedTypeAnnotation,
            )?;
            let ty = input.parse::<Type>()?;

            parameters.push(Parameter { name, ty });

            if let Some(Token {
                kind: TokenKind::Comma,
                ..
            }) = input.peek()
            {
                input.next()?;
            } else {
                break;
            }
        }

        input.expect(
            TokenKind::ParenClose,
            "the end of the parameters",
            ParserError::ExpectedParameterListEnd,
        )?;

        let explicit_return_ty = if let Some(Token {
            kind: TokenKind::Arrow,
            ..
        }) = input.peek()
        {
            input.next()?;
            Some(input.parse::<Type>()?)
        } else {
            None
        };

        let body = input.parse_with::<Expression<_>>(Precedence::Calling.as_u8())?;
        let span = fn_token.span + body.span().clone();

        Ok(Lambda {
            id: input.next_lambda_id(),
            parameters,
            explicit_return_ty,
            body: Box::new(body),
            span,
            ty: (),
        })
    }
}

impl Parse for Call<Untyped> {
    type Params = Expression<Untyped>;

    fn parse(input: &mut Parser, callee: Self::Params) -> Result<Self> {
        input.expect(
            TokenKind::ParenOpen,
            "a list of arguments",
            ParserError::ExpectedArgumentList,
        )?;

        let mut arguments = vec![];
        while let Some(token) = input.peek() {
            if token.kind == TokenKind::ParenClose {
                break;
            }

            arguments.push(input.parse_with::<Expression<_>>(0)?);

            if let Some(Token {
                kind: TokenKind::Comma,
                ..
            }) = input.peek()
            {
                input.next()?;
            } else {
                break;
            }
        }

        let close = input.expect(
            TokenKind::ParenClose,
            "the end of the arguments",
            ParserError::ExpectedArgumentListEnd,
        )?;

        let span = callee.span().clone() + close.span;

        Ok(Call {
            callee: Box::new(callee),
            arguments,
            span,
            ty: (),
        })
    }
}

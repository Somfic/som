use crate::{
    ast::{Binary, Expr, Expression, Group, Primary, Unary},
    lexer::{Token, TokenKind, TokenValue},
    parser::{Parse, ParsePhase},
    Parser, ParserError, Result,
};

impl Parse for Expression<ParsePhase> {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let (expr, span) = input.parse_with_span_with::<Expr<ParsePhase>>(min_bp)?;

        Ok(Expression { expr, span, ty: () })
    }
}

impl Parse for Binary<ParsePhase> {
    type Params = Expression<ParsePhase>;

    fn parse(input: &mut Parser, lhs: Self::Params) -> Result<Self> {
        let op = input.next()?;

        let Some(&(_, r_bp)) = input.lookup.binding_power_lookup.get(&op.kind) else {
            return ParserError::InvalidBinaryOperator
                .to_diagnostic()
                .with_label(op.span.label("expected a binary operator"))
                .to_err();
        };

        let rhs = input.parse_with(r_bp)?;

        match op.kind {
            TokenKind::Plus => Ok(Binary::Add(Box::new(lhs), Box::new(rhs))),
            TokenKind::Minus => Ok(Binary::Subtract(Box::new(lhs), Box::new(rhs))),
            TokenKind::Star => Ok(Binary::Multiply(Box::new(lhs), Box::new(rhs))),
            TokenKind::Slash => Ok(Binary::Divide(Box::new(lhs), Box::new(rhs))),
            _ => ParserError::InvalidBinaryOperator
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
                .to_err(),
        }
    }
}

impl Parse for Expr<ParsePhase> {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let peek = input.peek_expect("an expression")?;
        let peek_kind = peek.kind.clone();
        let peek_span = peek.span.clone();

        let Some(prefix) = input.lookup.expression_lookup.get(&peek_kind).cloned() else {
            return ParserError::InvalidPrimaryExpression
                .to_diagnostic()
                .with_label(peek_span.label("expected an expression"))
                .to_err();
        };

        let mut lhs = prefix(input)?;

        loop {
            let Some(token) = input.peek() else { break };
            let token_kind = token.kind.clone();

            let Some(infix) = input
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

            lhs = infix(input, lhs)?;
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

impl Parse for Unary<ParsePhase> {
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

impl Parse for Group<ParsePhase> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        input.expect(
            TokenKind::ParenOpen,
            "'(' to start a grouped expression",
            ParserError::ExpectedOpenParenthesis,
        )?;

        let expr = input.parse_with::<Expression<ParsePhase>>(0)?;

        input.expect(
            TokenKind::ParenClose,
            "')' to end a grouped expression",
            ParserError::ExpectedCloseParenthesis,
        )?;

        Ok(Group {
            expr: Box::new(expr),
        })
    }
}

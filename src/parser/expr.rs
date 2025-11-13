use crate::{
    ast::{Binary, Expr, Expression, Primary, Unary},
    lexer::{Token, TokenKind, TokenValue},
    parser::{infix_binding_power, Parse, ParsePhase},
    Error, Parser, Result,
};

impl Parse for Expression<ParsePhase> {
    type Params = u8;

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let (expr, span) = input.parse_with_span()?;

        Ok(Expression { expr, span, ty: () })
    }
}

impl Parse for Binary<ParsePhase> {
    type Params = Expression<ParsePhase>; // Takes the LHS as a parameter

    fn parse(input: &mut Parser, lhs: Self::Params) -> Result<Self> {
        let op_token = input.lexer.next().unwrap()?; // consume operator

        let (_, r_bp) = infix_binding_power(&op_token.kind).ok_or_else(|| {
            Error::ParserError(format!("expected binary operator, found {}", op_token.kind))
        })?;

        let rhs = input.parse_with::<Expression<ParsePhase>>(r_bp)?;

        let binary = match op_token.kind {
            TokenKind::Plus => Binary::Add(Box::new(lhs), Box::new(rhs)),
            _ => {
                return Error::ParserError(format!(
                    "unexpected binary operator: {}",
                    op_token.kind
                ))
                .to_diagnostic()
                .to_err();
            }
        };

        Ok(binary)
    }
}

impl Parse for Expr<ParsePhase> {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let mut lhs = match input.peek_expect()?.kind {
            TokenKind::Minus | TokenKind::Bang => {
                let (inner, span) = input.parse_with_span()?;
                Expression {
                    expr: Expr::Unary(inner),
                    span,
                    ty: (),
                }
            }
            _ => {
                let (inner, span) = input.parse_with_span()?;
                Expression {
                    expr: Expr::Primary(inner),
                    span,
                    ty: (),
                }
            }
        };

        while let Ok(token) = input.next() {
            lhs = match token.kind {
                TokenKind::Minus | TokenKind::Plus => {
                    let (l_bp, _) = infix_binding_power(&token.kind).unwrap();
                    if l_bp < min_bp {
                        break;
                    }
                    let (expr, span) = input.parse_with_span_with(0)?;
                    let rhs = Expression { expr, span, ty: () };

                    Expression {
                        span: &rhs.span + &lhs.span,
                        expr: Expr::Binary(Binary::Add(Box::new(lhs), Box::new(rhs))),
                        ty: (),
                    }
                }
                _ => break,
            }
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
            token => Error::ParserError(format!("expected literal, found {}", token.kind))
                .to_diagnostic()
                .with_label(token.span.label("expected this to be a literal"))
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
            _ => Error::ParserError(format!("expected unary, got {}", input.peek_expect()?.kind))
                .to_diagnostic()
                .with_label(token.span.label("expected this to be a unary operator"))
                .to_err(),
        }
    }
}

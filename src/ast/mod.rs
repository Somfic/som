use crate::lexer::{self, Token, TokenKind, TokenValue};
use crate::parser::{Parse, Parser};
use crate::{Error, Result};

pub struct Parsing;

pub trait PhaseInfo {
    type TypeInfo;
}

impl PhaseInfo for Parsing {
    type TypeInfo = (); // No type info during parsing
}

#[derive(Debug)]
pub struct AstExpression<Phase: PhaseInfo = Parsing> {
    pub kind: Expr,
    pub span: lexer::Span,
    pub ty: Phase::TypeInfo,
}

#[derive(Debug)]
pub enum Expr {
    Primary(Primary),
    Unary(Unary),
    Binary(Binary),
}

impl Parse for Binary {
    type Params = Expr; // Takes the LHS as a parameter

    fn parse(input: &mut Parser, lhs: Self::Params) -> Result<Self> {
        let op_token = input.lexer.next().unwrap()?; // consume operator

        let (_, r_bp) = infix_binding_power(&op_token.kind).ok_or_else(|| {
            Error::ParserError(format!("expected binary operator, found {}", op_token.kind))
        })?;

        let rhs = input.parse_with::<Expr>(r_bp)?;

        let binary = match op_token.kind {
            TokenKind::Plus => Binary::Add(Box::new(lhs), Box::new(rhs)),
            _ => {
                return Err(Error::ParserError(format!(
                    "unexpected binary operator: {}",
                    op_token.kind
                )))
            }
        };

        Ok(binary)
    }
}

impl Parse for Expr {
    type Params = u8;

    fn parse(input: &mut Parser, min_bp: Self::Params) -> Result<Self> {
        let mut lhs = match input.peek_expect()?.kind {
            TokenKind::Minus | TokenKind::Bang => Expr::Unary(input.parse()?),
            _ => Expr::Primary(input.parse()?),
        };

        while let Some(token) = input.peek() {
            lhs = match token.kind {
                TokenKind::Minus | TokenKind::Plus => {
                    let (l_bp, _) = infix_binding_power(&token.kind).unwrap();
                    if l_bp < min_bp {
                        break;
                    }
                    Expr::Binary(input.parse_with(lhs)?)
                }
                _ => break,
            }
        }

        Ok(lhs)
    }
}

fn infix_binding_power(kind: &TokenKind) -> Option<(u8, u8)> {
    Some(match kind {
        TokenKind::Plus | TokenKind::Minus => (9, 10),
        TokenKind::Star | TokenKind::Slash => (11, 12),
        _ => return None,
    })
}

impl Parse for Primary {
    type Params = ();

    fn parse(input: &mut Parser, _: Self::Params) -> Result<Self> {
        match input.next()? {
            Token {
                kind: TokenKind::Boolean,
                value: TokenValue::Boolean(b),
                ..
            } => Ok(Primary::Boolean(b)),
            token => Err(crate::Error::ParserError(format!(
                "expected literal, found {}",
                token.kind
            ))),
        }
    }
}

#[derive(Debug)]
pub enum Primary {
    Boolean(bool),
    I32(i32),
    I64(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
}

#[derive(Debug)]
pub enum Unary {
    Negate(Box<Expr>),
}

#[derive(Debug)]
pub enum Binary {
    Add(Box<Expr>, Box<Expr>),
}

impl Parse for Unary {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        match input.peek_expect()?.kind {
            TokenKind::Minus => {
                todo!()
            }
            _ => Err(Error::ParserError(format!(
                "expected unary, got {}",
                input.peek_expect()?.kind
            ))),
        }
    }
}

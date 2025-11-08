use crate::lexer::{self, Lexer};
use crate::Result;

pub struct Parsing;

pub trait PhaseInfo {
    type TypeInfo;
}

impl PhaseInfo for Parsing {
    type TypeInfo = (); // No type info during parsing
}

pub struct AstExpression<Phase: PhaseInfo = Parsing> {
    pub kind: Expr<Phase>,
    pub span: lexer::Span,
    pub ty: Phase::TypeInfo,
}

pub enum Expr<Phase: PhaseInfo> {
    ExprLiteral(Literal<Phase>),
}

trait Parse: Sized {
    fn parse(input: Lexer) -> Result<Self>;
}

struct Literal<Phase: PhaseInfo> {
    pub value: LiteralValue,
    pub span: lexer::Span,
    pub ty: Phase::TypeInfo,
}

pub enum LiteralValue {
    Boolean(bool),
    I32(i32),
    I64(i64),
    Decimal(f64),
    String(Box<str>),
    Character(char),
}

impl 

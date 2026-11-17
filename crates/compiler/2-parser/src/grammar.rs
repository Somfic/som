//! Which tokens do what: binding powers for the Pratt parser, and the sets of tokens that
//! decide recovery and the fixes for a missing token.

use som_ast::BinOp;
use som_lexer::TokenKind;

use crate::ty::primitive;

/// A level `p` binds at `2p` on one side and `2p + 1` on the other, which side making it
/// left- or right-associative.
pub(crate) struct Precedence;

impl Precedence {
    /// `=`, and `then if condition else otherwise`.
    pub const ASSIGNMENT: u8 = 0;
    /// `optional else fallback`: below every binary operator, above `if … else` and `=`.
    pub const FALLBACK: u8 = 1;
    pub const OR: u8 = 2;
    pub const AND: u8 = 3;
    /// Below comparisons and arithmetic, so `0..n + 1` is `0..(n + 1)`.
    pub const RANGE: u8 = 4;
    pub const EQUALITY: u8 = 5;
    pub const COMPARISON: u8 = 6;
    /// The bitwise operators bind tighter than comparisons, unlike C, so `x & 1 == 0` is
    /// `(x & 1) == 0`.
    pub const BIT_OR: u8 = 7;
    pub const BIT_XOR: u8 = 8;
    pub const BIT_AND: u8 = 9;
    pub const SHIFT: u8 = 10;
    pub const ADDITIVE: u8 = 11;
    pub const MULTIPLICATIVE: u8 = 12;
    /// Tighter than `*`, looser than unary operators, so `-x as u8` converts `-x`.
    pub const AS: u8 = 13;
    pub const UNARY: u8 = 14;
    pub const POSTFIX: u8 = 15;
}

/// The binding powers on an operator's left and right.
#[derive(Clone, Copy)]
pub(crate) struct Bp {
    pub left: u8,
    pub right: u8,
}

impl Bp {
    const fn left_assoc(precedence: u8) -> Bp {
        Bp {
            left: precedence * 2,
            right: precedence * 2 + 1,
        }
    }

    const fn right_assoc(precedence: u8) -> Bp {
        Bp {
            left: precedence * 2 + 1,
            right: precedence * 2,
        }
    }

    const fn non_assoc(precedence: u8) -> Bp {
        Bp {
            left: precedence * 2,
            right: precedence * 2,
        }
    }

    pub const ASSIGN: Bp = Bp::right_assoc(Precedence::ASSIGNMENT);
    pub const CONDITIONAL: Bp = Bp::ASSIGN;
    /// `a else b else c` tries `a`, then `b`, then takes `c`.
    pub const FALLBACK: Bp = Bp::right_assoc(Precedence::FALLBACK);
    /// A conditional's condition stops before the `else` that belongs to it.
    pub const CONDITION: u8 = Bp::FALLBACK.left + 1;
    pub const RANGE: Bp = Bp::left_assoc(Precedence::RANGE);
    pub const SHIFT: Bp = Bp::left_assoc(Precedence::SHIFT);
    pub const AS: u8 = Bp::left_assoc(Precedence::AS).left;
    pub const PREFIX: u8 = Bp::right_assoc(Precedence::UNARY).left;
    pub const POSTFIX: u8 = Bp::left_assoc(Precedence::POSTFIX).left;
}

pub(crate) fn infix_op(kind: TokenKind) -> Option<(BinOp, Bp)> {
    let left = Bp::left_assoc;
    Some(match kind {
        TokenKind::PipePipe => (BinOp::Or, left(Precedence::OR)),
        TokenKind::AndAnd => (BinOp::And, left(Precedence::AND)),
        TokenKind::DoubleEquals => (BinOp::Equals, left(Precedence::EQUALITY)),
        TokenKind::NotEquals => (BinOp::NotEquals, left(Precedence::EQUALITY)),
        TokenKind::LessThan => (BinOp::LessThan, Bp::non_assoc(Precedence::COMPARISON)),
        TokenKind::GreaterThan => (BinOp::GreaterThan, Bp::non_assoc(Precedence::COMPARISON)),
        TokenKind::LessThanOrEqual => (
            BinOp::LessThanOrEqual,
            Bp::non_assoc(Precedence::COMPARISON),
        ),
        TokenKind::GreaterThanOrEqual => (
            BinOp::GreaterThanOrEqual,
            Bp::non_assoc(Precedence::COMPARISON),
        ),
        TokenKind::Plus => (BinOp::Add, left(Precedence::ADDITIVE)),
        TokenKind::Minus => (BinOp::Subtract, left(Precedence::ADDITIVE)),
        TokenKind::Star => (BinOp::Multiply, left(Precedence::MULTIPLICATIVE)),
        TokenKind::Slash => (BinOp::Divide, left(Precedence::MULTIPLICATIVE)),
        // With `*` and `/`, left to right: `2 * 7 % 4` is `(2 * 7) % 4`.
        TokenKind::Percentage => (BinOp::Modulo, left(Precedence::MULTIPLICATIVE)),
        TokenKind::Pipe => (BinOp::BitOr, left(Precedence::BIT_OR)),
        TokenKind::Caret => (BinOp::BitXor, left(Precedence::BIT_XOR)),
        TokenKind::Ampersand => (BinOp::BitAnd, left(Precedence::BIT_AND)),
        _ => return None,
    })
}

/// `<<` and `>>` are two `<` or `>` right next to each other: one token would end
/// `List<List<i32>>` too soon.
pub(crate) fn shift_op(kind: TokenKind) -> Option<BinOp> {
    match kind {
        TokenKind::LessThan => Some(BinOp::ShiftLeft),
        TokenKind::GreaterThan => Some(BinOp::ShiftRight),
        _ => None,
    }
}

pub(crate) fn is_prefix_op(kind: TokenKind) -> bool {
    PREFIX_OPS.contains(&kind)
}

const PREFIX_OPS: &[TokenKind] = &[
    TokenKind::Bang,
    TokenKind::Minus,
    TokenKind::Ampersand,
    TokenKind::Star,
    TokenKind::Tilde,
];

pub(crate) const STATEMENT_SYNC: &[TokenKind] = &[
    TokenKind::Semicolon,
    TokenKind::CloseBrace,
    TokenKind::Let,
    TokenKind::Loop,
    TokenKind::While,
    TokenKind::Break,
    TokenKind::If,
    TokenKind::Fn,
];

pub(crate) const MEMBER_SYNC: &[TokenKind] = &[
    TokenKind::Fn,
    TokenKind::Prop,
    TokenKind::Type,
    TokenKind::CloseBrace,
];

pub(crate) const DECLARATION_SYNC: &[TokenKind] = &[
    TokenKind::Fn,
    TokenKind::Extern,
    TokenKind::Struct,
    TokenKind::Enum,
    TokenKind::Trait,
    TokenKind::Impl,
];

pub(crate) const ITEM_KEYWORDS: &[TokenKind] = &[
    TokenKind::Fn,
    TokenKind::Extern,
    TokenKind::Struct,
    TokenKind::Enum,
    TokenKind::Trait,
    TokenKind::Use,
    TokenKind::Impl,
    TokenKind::Type,
];

pub(crate) const VISIBILITY: &[TokenKind] = &[TokenKind::Pub, TokenKind::Mod];

/// Tokens a statement starts with, before which a missing `;` makes sense.
pub(crate) const STATEMENT_START: &[TokenKind] = &[
    TokenKind::Let,
    TokenKind::Return,
    TokenKind::If,
    TokenKind::While,
    TokenKind::For,
    TokenKind::Loop,
    TokenKind::Match,
    TokenKind::Break,
    TokenKind::Continue,
    TokenKind::CloseBrace,
];

pub(crate) const CLOSERS: &[TokenKind] = &[
    TokenKind::CloseParen,
    TokenKind::CloseBracket,
    TokenKind::CloseBrace,
];

/// Before which a missing closing bracket makes sense.
pub(crate) const AFTER_CLOSER: &[TokenKind] = &[
    TokenKind::Semicolon,
    TokenKind::OpenBrace,
    TokenKind::CloseParen,
    TokenKind::CloseBracket,
    TokenKind::CloseBrace,
];

/// Where an expression can't start: after `..`, a range without an end.
pub(crate) const ENDS_EXPR: &[TokenKind] = &[
    TokenKind::CloseParen,
    TokenKind::CloseBracket,
    TokenKind::CloseBrace,
    TokenKind::OpenBrace,
    TokenKind::Comma,
    TokenKind::Semicolon,
    TokenKind::Else,
    TokenKind::Eof,
];

/// The text a missing `kind` is put in as, for punctuation, which is always written the
/// same way: a forgotten `;`, `,` or `)`.
pub(crate) fn insertion_text(kind: TokenKind) -> Option<&'static str> {
    Some(match kind {
        TokenKind::Semicolon => ";",
        TokenKind::Comma => ",",
        TokenKind::Colon => ":",
        TokenKind::CloseParen => ")",
        TokenKind::CloseBracket => "]",
        TokenKind::CloseBrace => "}",
        TokenKind::GreaterThan => ">",
        TokenKind::FatArrow => " =>",
        TokenKind::Equals => " =",
        _ => return None,
    })
}

/// The text of `missing` when `found` is a look-alike of it: `->` for `=>`.
pub(crate) fn look_alike(missing: TokenKind, found: TokenKind) -> Option<&'static str> {
    match (missing, found) {
        (TokenKind::FatArrow, TokenKind::Arrow) => Some("=>"),
        (TokenKind::Arrow, TokenKind::FatArrow) => Some("->"),
        _ => None,
    }
}

/// Whether an item of a list (an expression, a field, a parameter, a type) can start with
/// `kind`: what tells a missing `,` from a missing closing bracket.
pub(crate) fn can_start_item(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident
            | TokenKind::Int
            | TokenKind::Float
            | TokenKind::Text
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None
            | TokenKind::SelfValue
            | TokenKind::SelfType
            | TokenKind::Mut
            | TokenKind::OpenParen
            | TokenKind::OpenBracket
            | TokenKind::Pub
            | TokenKind::Mod
    ) || is_prefix_op(kind)
        || primitive(kind).is_some()
}

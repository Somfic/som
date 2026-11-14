use som_ast::BinOp;
use som_lexer::TokenKind;

// `Right` is reserved for right-associative operators.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub enum Association {
    Left,
    Right,
    None,
}

pub struct OpInfo {
    pub precedence: u8,
    pub association: Association,
}

impl OpInfo {
    pub fn binding_power(self) -> (u8, u8) {
        let base = self.precedence * 2;
        match self.association {
            Association::Left => (base, base + 1),
            Association::Right => (base + 1, base),
            Association::None => (base, base),
        }
    }
}

pub struct Grammar;

#[allow(dead_code)]
impl Grammar {
    pub const ASSIGNMENT: u8 = 1;
    /// `optional else fallback`: below every binary operator, above `if … else` and `=`.
    /// Right-associative: `a else b else c` tries `a`, then `b`, then takes `c`.
    pub const FALLBACK: u8 = 3;
    pub const OR: u8 = 2;
    pub const AND: u8 = 3;
    /// `a..b`: below comparisons and arithmetic, so `0..n + 1` is `0..(n + 1)`.
    pub const RANGE: u8 = 4;
    pub const EQUALITY: u8 = 5;
    pub const COMPARISON: u8 = 6;
    /// `|`, `^`, `&`, then `<<` and `>>`: tighter than comparisons, unlike C, so
    /// `x & 1 == 0` is `(x & 1) == 0`.
    pub const BIT_OR: u8 = 7;
    pub const BIT_XOR: u8 = 8;
    pub const BIT_AND: u8 = 9;
    pub const SHIFT: u8 = 10;
    pub const ADDITIVE: u8 = 11;
    pub const MULTIPLICATIVE: u8 = 12;
    /// `x as T`: tighter than `*`, looser than unary operators, so `-x as u8` converts `-x`.
    pub const AS: u8 = 13;
    pub const UNARY: u8 = 14;
    pub const POSTFIX: u8 = 15;

    pub fn infix_op(kind: TokenKind) -> Option<(BinOp, OpInfo)> {
        let (operator, precedence, association) = match kind {
            TokenKind::Or => (BinOp::Or, Self::OR, Association::Left),
            TokenKind::And => (BinOp::And, Self::AND, Association::Left),
            TokenKind::DoubleEquals => (BinOp::Equals, Self::EQUALITY, Association::Left),
            TokenKind::NotEquals => (BinOp::NotEquals, Self::EQUALITY, Association::Left),

            TokenKind::LessThan => (BinOp::LessThan, Self::COMPARISON, Association::None),
            TokenKind::GreaterThan => (BinOp::GreaterThan, Self::COMPARISON, Association::None),
            TokenKind::LessThanOrEqual => {
                (BinOp::LessThanOrEqual, Self::COMPARISON, Association::None)
            }
            TokenKind::GreaterThanOrEqual => (
                BinOp::GreaterThanOrEqual,
                Self::COMPARISON,
                Association::None,
            ),

            TokenKind::Plus => (BinOp::Add, Self::ADDITIVE, Association::Left),
            TokenKind::Minus => (BinOp::Subtract, Self::ADDITIVE, Association::Left),

            TokenKind::Star => (BinOp::Multiply, Self::MULTIPLICATIVE, Association::Left),
            TokenKind::Slash => (BinOp::Divide, Self::MULTIPLICATIVE, Association::Left),
            // With `*` and `/`, left to right: `2 * 7 % 4` is `(2 * 7) % 4`.
            TokenKind::Percentage => (BinOp::Modulo, Self::MULTIPLICATIVE, Association::Left),

            TokenKind::Pipe => (BinOp::BitOr, Self::BIT_OR, Association::Left),
            TokenKind::Caret => (BinOp::BitXor, Self::BIT_XOR, Association::Left),
            TokenKind::Ampersand => (BinOp::BitAnd, Self::BIT_AND, Association::Left),

            _ => return None,
        };

        Some((
            operator,
            OpInfo {
                precedence,
                association,
            },
        ))
    }

    pub fn prefix_bp(kind: TokenKind) -> Option<u8> {
        match kind {
            TokenKind::Bang
            | TokenKind::Minus
            | TokenKind::Ampersand
            | TokenKind::Star
            | TokenKind::Tilde => Some(Self::UNARY * 2 + 1),
            _ => None,
        }
    }
}

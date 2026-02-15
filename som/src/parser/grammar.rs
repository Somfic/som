use crate::{BinOp, lexer::TokenKind};

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

impl Grammar {
    pub const ASSIGNMENT: u8 = 1;
    pub const OR: u8 = 2;
    pub const AND: u8 = 3;
    pub const EQUALITY: u8 = 4;
    pub const COMPARISON: u8 = 5;
    pub const ADDITIVE: u8 = 6;
    pub const MULTIPLICATIVE: u8 = 7;
    pub const UNARY: u8 = 8;
    pub const POSTFIX: u8 = 9;

    pub fn infix_op(kind: TokenKind) -> Option<(BinOp, OpInfo)> {
        let (operator, precedence, association) = match kind {
            // TokenKind::Equals => (BinOp::Assign, Self::ASSIGNMENT, Assoc::Right),
            // TokenKind::Or => (BinOp::Or, Self::OR, Assoc::Left),
            // TokenKind::AmpAmp => (BinOp::And, Self::AND, Assoc::Left),
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
            TokenKind::Bang | TokenKind::Minus | TokenKind::Ampersand | TokenKind::Star => {
                Some(Self::UNARY * 2 + 1)
            }
            _ => None,
        }
    }
}

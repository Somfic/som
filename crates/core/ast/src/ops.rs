use crate::BinOp;
use som_lang::LangTrait;

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Subtract => "-",
            BinOp::Multiply => "*",
            BinOp::Divide => "/",
            BinOp::Modulo => "%",
            BinOp::LessThan => "<",
            BinOp::GreaterThan => ">",
            BinOp::LessThanOrEqual => "<=",
            BinOp::GreaterThanOrEqual => ">=",
            BinOp::Equals => "==",
            BinOp::NotEquals => "!=",
            BinOp::And => "and",
            BinOp::Or => "or",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::ShiftLeft => "<<",
            BinOp::ShiftRight => ">>",
        }
    }

    pub fn lang_trait(self) -> Option<LangTrait> {
        Some(match self {
            BinOp::Equals | BinOp::NotEquals => LangTrait::Eq,
            BinOp::Add => LangTrait::Add,
            BinOp::Subtract => LangTrait::Subtract,
            BinOp::Multiply => LangTrait::Multiply,
            BinOp::Divide => LangTrait::Divide,
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => LangTrait::Compare,
            BinOp::Modulo
            | BinOp::And
            | BinOp::Or
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::ShiftLeft
            | BinOp::ShiftRight => return None,
        })
    }

    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
        ) || self.is_bitwise()
    }

    pub fn is_bitwise(self) -> bool {
        matches!(
            self,
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight
        )
    }

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::LessThan
                | BinOp::GreaterThan
                | BinOp::LessThanOrEqual
                | BinOp::GreaterThanOrEqual
                | BinOp::Equals
                | BinOp::NotEquals
        )
    }
}

use ena::unify::{UnifyKey, UnifyValue};
use som_common::{Span, expand_enum};

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Type {
        Error,
        I32,
        Bool,
        Infer { var: TypeVar },
    } with { span: Span }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar(u32);

impl UnifyKey for TypeVar {
    type Value = TypeValue;
    fn index(&self) -> u32 {
        self.0
    }
    fn from_index(u: u32) -> Self {
        TypeVar(u)
    }
    fn tag() -> &'static str {
        "TypeVar"
    }
}

impl UnifyValue for TypeValue {
    type Error = ();
    fn unify_values(a: &Self, b: &Self) -> Result<Self, ()> {
        use TypeValue::*;
        Ok(match (a, b) {
            (I32, I32) => I32,
            (Bool, Bool) => Bool,
            (I32, Bool) | (Bool, I32) => return Err(()),
            (I32, Unbound { .. }) | (Unbound { .. }, I32) => I32,
            (Bool, Unbound { is_int: true }) | (Unbound { is_int: true }, Bool) => return Err(()),
            (Bool, Unbound { .. }) | (Unbound { .. }, Bool) => Bool,
            (Unbound { is_int: x }, Unbound { is_int: y }) => Unbound { is_int: *x || *y },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeValue {
    Unbound { is_int: bool },
    I32,
    Bool,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Type::I32 { .. } => "i32",
            Type::Bool { .. } => "bool",
            Type::Error { .. } => "<error>",
            Type::Infer { .. } => "<inferred>",
        })
    }
}

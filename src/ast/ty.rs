use std::fmt::Display;

use ena::unify::{UnifyKey, UnifyValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Unknown(TypeVar),
    Named(Box<str>),
    I32,
    U8,
    F32,
    Bool,
    Str,
    Reference {
        mutable: bool,
        lifetime: Lifetime,
        to: Box<Type>,
    },
    Fun {
        arguments: Vec<Type>,
        returns: Box<Type>,
    },
}

impl Type {
    pub fn is_copy(&self) -> bool {
        matches!(
            self,
            Type::Unit | Type::Bool | Type::I32 | Type::U8 | Type::F32 | Type::Reference { .. }
        )
    }

    pub fn size(&self) -> usize {
        match self {
            Type::Unit => 0,
            Type::Bool | Type::U8 => 1,
            Type::I32 | Type::F32 => 4,
            Type::Str => std::mem::size_of::<&str>(),
            Type::Reference { .. } => std::mem::size_of::<&()>(),
            Type::Unknown(_) | Type::Named(_) | Type::Fun { .. } => {
                panic!("Size of unknown/named/fun type is not known at compile time")
            }
        }
    }

    pub fn alignment(&self) -> usize {
        match self {
            Type::Unit => 1,
            Type::Bool | Type::U8 => 1,
            Type::I32 | Type::F32 => 4,
            Type::Str => std::mem::align_of::<&str>(),
            Type::Reference { .. } => std::mem::align_of::<&()>(),
            Type::Unknown(_) | Type::Named(_) | Type::Fun { .. } => {
                panic!("Alignment of unknown/named/fun type is not known at compile time")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar(pub u32);

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeValue {
    Bound(Type),
    Unbound,
}

impl UnifyValue for TypeValue {
    type Error = ();

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        match (a, b) {
            // Both unbound - stay unbound
            (TypeValue::Unbound, TypeValue::Unbound) => Ok(TypeValue::Unbound),

            // One bound, one unbound - take the bound one
            (TypeValue::Bound(ty), TypeValue::Unbound)
            | (TypeValue::Unbound, TypeValue::Bound(ty)) => Ok(TypeValue::Bound(ty.clone())),

            // Both bound - must be identical
            (TypeValue::Bound(t1), TypeValue::Bound(t2)) => {
                if t1 == t2 {
                    Ok(TypeValue::Bound(t1.clone()))
                } else {
                    Err(()) // Type mismatch
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifetimeVar(pub u32);

impl UnifyKey for LifetimeVar {
    type Value = LifetimeValue;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        LifetimeVar(u)
    }

    fn tag() -> &'static str {
        "LifetimeVar"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifetimeValue {
    Bound(Lifetime),
    Unbound,
}

impl UnifyValue for LifetimeValue {
    type Error = ();

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        match (a, b) {
            // Both unbound - stay unbound
            (LifetimeValue::Unbound, LifetimeValue::Unbound) => Ok(LifetimeValue::Unbound),

            // One bound, one unbound - take the bound one
            (LifetimeValue::Bound(lt), LifetimeValue::Unbound)
            | (LifetimeValue::Unbound, LifetimeValue::Bound(lt)) => {
                Ok(LifetimeValue::Bound(lt.clone()))
            }

            // Both bound - must be identical
            (LifetimeValue::Bound(l1), LifetimeValue::Bound(l2)) => {
                if l1 == l2 {
                    Ok(LifetimeValue::Bound(l1.clone()))
                } else {
                    Err(()) // Lifetime mismatch
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifetime {
    Unknown(LifetimeVar),
    Unspecified,
    Named(Box<str>),
    Static,
}

impl Display for Lifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lifetime::Unknown(var) => write!(f, "'{}", var.0),
            Lifetime::Unspecified => write!(f, "'_"),
            Lifetime::Named(name) => write!(f, "'{}", name),
            Lifetime::Static => write!(f, "'static"),
        }
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit => write!(f, "()"),
            Type::Unknown(var) => write!(f, "T{}", var.0),
            Type::Named(name) => write!(f, "{}", name),
            Type::Bool => write!(f, "bool"),
            Type::I32 => write!(f, "i32"),
            Type::U8 => write!(f, "u8"),
            Type::F32 => write!(f, "f32"),
            Type::Str => write!(f, "str"),
            Type::Reference {
                mutable,
                lifetime,
                to,
            } => {
                let lifetime = if let Lifetime::Unspecified = lifetime {
                    ""
                } else {
                    &format!("{} ", lifetime)
                };

                if *mutable {
                    write!(f, "&mut{}{}", lifetime, to)
                } else {
                    write!(f, "&{}{}", lifetime, to)
                }
            }
            Type::Fun { arguments, returns } => {
                let args: Vec<String> = arguments.iter().map(|arg| format!("{}", arg)).collect();
                write!(f, "fn({}) -> {}", args.join(", "), returns)
            }
        }
    }
}

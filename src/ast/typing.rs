use std::fmt::Display;

use cranelift::prelude::types;

use crate::{ast::Pseudo, Span};

#[derive(Debug, Clone)]
pub struct Type {
    pub kind: TypeKind,
    pub span: Span,
}

impl TypeKind {
    pub fn with_span(self, span: &Span) -> Type {
        Type {
            kind: self,
            span: span.clone(),
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeKind {
    Unit,
    Boolean,
    I32,
    I64,
    Decimal,
    String,
    Character,
    Function {
        parameters: Vec<Type>,
        returns: Box<Type>,
    },
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl Display for TypeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeKind::Unit => write!(f, "a unit type"),
            TypeKind::Boolean => write!(f, "a boolean"),
            TypeKind::I32 => write!(f, "a 32-bit integer"),
            TypeKind::I64 => write!(f, "a 64-bit integer"),
            TypeKind::Decimal => write!(f, "a decimal"),
            TypeKind::String => write!(f, "a string"),
            TypeKind::Character => write!(f, "a character"),
            TypeKind::Function {
                parameters: params,
                returns,
            } => {
                write!(f, "(")?;
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param.pseudo())?;
                }
                write!(f, ") -> {}", returns.pseudo())
            }
        }
    }
}

impl Pseudo for Type {
    fn pseudo(&self) -> String {
        format!("{}", self.kind.pseudo())
    }
}

impl Pseudo for TypeKind {
    fn pseudo(&self) -> String {
        match self {
            TypeKind::Unit => "unit".to_string(),
            TypeKind::Boolean => "bool".to_string(),
            TypeKind::I32 => "i32".to_string(),
            TypeKind::I64 => "i64".to_string(),
            TypeKind::Decimal => "decimal".to_string(),
            TypeKind::String => "string".to_string(),
            TypeKind::Character => "char".to_string(),
            TypeKind::Function {
                parameters: params,
                returns,
            } => {
                let mut s = String::from("fn(");
                for (i, param) in params.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&param.pseudo());
                }
                s.push_str(") -> ");
                s.push_str(&returns.pseudo());
                s
            }
        }
    }
}

impl From<Type> for cranelift::prelude::Type {
    fn from(value: Type) -> Self {
        value.kind.into()
    }
}

impl From<TypeKind> for cranelift::prelude::Type {
    fn from(val: TypeKind) -> Self {
        match val {
            TypeKind::Unit => types::I8,
            TypeKind::Boolean => types::I8,
            TypeKind::I32 => types::I32,
            TypeKind::I64 => types::I64,
            TypeKind::Decimal => types::F64,
            TypeKind::String => todo!(),
            TypeKind::Character => todo!(),
            TypeKind::Function { .. } => types::I64, // a pointer
        }
    }
}

use std::{collections::HashMap, fmt::Display};

use cranelift::prelude::types;

use crate::{
    ast::{Expression, Pseudo},
    parser::Untyped,
    Phase, Result, Span, TypeCheckError,
};

mod expression;
mod statement;

#[derive(Debug)]
pub struct Typed;

impl Phase for Typed {
    type TypeInfo = Type;
}

pub struct Typer {}

impl Typer {
    pub fn new() -> Self {
        Self {}
    }

    // pub fn check(&mut self, expression: Expression<Untyped>) -> Result<Expression<Typed>> {
    //     expression
    //         .type_check(&mut TypeCheckContext {})
    //         .map(|(e, _)| e)
    // }

    pub fn check(&mut self, expression: Expression<Untyped>) -> Result<Expression<Typed>> {
        expression.type_check(&mut TypeCheckContext::new())
    }
}

pub trait TypeCheck: Sized {
    type Output;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output>;
}

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

pub struct TypeCheckContext<'a> {
    parent: Option<&'a TypeCheckContext<'a>>,
    variables: HashMap<String, Type>,
}

impl<'a> TypeCheckContext<'a> {
    pub fn new() -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
        }
    }

    pub fn get_variable(&self, name: impl Into<String>) -> Result<Type> {
        let name = name.into();

        self.variables
            .get(&name)
            .cloned()
            .or_else(|| {
                self.parent
                    .and_then(|parent_ctx| parent_ctx.get_variable(name).ok())
            })
            .ok_or_else(|| TypeCheckError::UndefinedVariable.to_diagnostic())
    }

    pub fn declare_variable(&mut self, name: impl Into<String>, ty: Type) {
        self.variables.insert(name.into(), ty);
    }

    fn new_child_context(&self) -> TypeCheckContext<'_> {
        TypeCheckContext {
            parent: Some(self),
            variables: HashMap::new(),
        }
    }
}

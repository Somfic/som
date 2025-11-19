use std::fmt::Display;

use cranelift::prelude::types;

use crate::{ast::Pseudo, lexer::Identifier, Span};

#[derive(Debug, Clone)]
pub enum Type {
    Unit(UnitType),
    Boolean(BooleanType),
    I32(I32Type),
    I64(I64Type),
    Decimal(DecimalType),
    String(StringType),
    Character(CharacterType),
    Function(FunctionType),
    Struct(StructType),
}

#[derive(Debug, Clone)]
pub struct UnitType {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct BooleanType {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct I32Type {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct I64Type {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DecimalType {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StringType {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct CharacterType {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct FunctionType {
    pub parameters: Vec<Type>,
    pub returns: Box<Type>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct StructType {
    pub name: Option<Identifier>,
    pub fields: Vec<StructField>,
    pub span: Span,
}

impl Type {
    pub fn span(&self) -> &Span {
        match self {
            Type::Unit(t) => &t.span,
            Type::Boolean(t) => &t.span,
            Type::I32(t) => &t.span,
            Type::I64(t) => &t.span,
            Type::Decimal(t) => &t.span,
            Type::String(t) => &t.span,
            Type::Character(t) => &t.span,
            Type::Function(t) => &t.span,
            Type::Struct(t) => &t.span,
        }
    }

    // Helper constructors
    pub fn unit(span: Span) -> Self {
        Type::Unit(UnitType { span })
    }

    pub fn boolean(span: Span) -> Self {
        Type::Boolean(BooleanType { span })
    }

    pub fn i32(span: Span) -> Self {
        Type::I32(I32Type { span })
    }

    pub fn i64(span: Span) -> Self {
        Type::I64(I64Type { span })
    }

    pub fn decimal(span: Span) -> Self {
        Type::Decimal(DecimalType { span })
    }

    pub fn string(span: Span) -> Self {
        Type::String(StringType { span })
    }

    pub fn character(span: Span) -> Self {
        Type::Character(CharacterType { span })
    }

    pub fn function(parameters: Vec<Type>, returns: Box<Type>, span: Span) -> Self {
        Type::Function(FunctionType {
            parameters,
            returns,
            span,
        })
    }

    pub fn struct_type(name: Option<Identifier>, fields: Vec<StructField>, span: Span) -> Self {
        Type::Struct(StructType { name, fields, span })
    }

    // Clone type with a new span (useful for error messages)
    pub fn with_span(&self, span: &Span) -> Self {
        match self {
            Type::Unit(_) => Type::unit(span.clone()),
            Type::Boolean(_) => Type::boolean(span.clone()),
            Type::I32(_) => Type::i32(span.clone()),
            Type::I64(_) => Type::i64(span.clone()),
            Type::Decimal(_) => Type::decimal(span.clone()),
            Type::String(_) => Type::string(span.clone()),
            Type::Character(_) => Type::character(span.clone()),
            Type::Function(f) => {
                Type::function(f.parameters.clone(), f.returns.clone(), span.clone())
            }
            Type::Struct(s) => Type::struct_type(s.name.clone(), s.fields.clone(), span.clone()),
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Type::Unit(_), Type::Unit(_)) => true,
            (Type::Boolean(_), Type::Boolean(_)) => true,
            (Type::I32(_), Type::I32(_)) => true,
            (Type::I64(_), Type::I64(_)) => true,
            (Type::Decimal(_), Type::Decimal(_)) => true,
            (Type::String(_), Type::String(_)) => true,
            (Type::Character(_), Type::Character(_)) => true,
            (Type::Function(a), Type::Function(b)) => {
                a.parameters == b.parameters && a.returns == b.returns
            }
            (Type::Struct(a), Type::Struct(b)) => {
                // Nominal typing: compare names
                a.name == b.name
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: Identifier,
    pub ty: Type,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit(t) => write!(f, "{}", t),
            Type::Boolean(t) => write!(f, "{}", t),
            Type::I32(t) => write!(f, "{}", t),
            Type::I64(t) => write!(f, "{}", t),
            Type::Decimal(t) => write!(f, "{}", t),
            Type::String(t) => write!(f, "{}", t),
            Type::Character(t) => write!(f, "{}", t),
            Type::Function(t) => write!(f, "{}", t),
            Type::Struct(t) => write!(f, "{}", t),
        }
    }
}

impl Display for UnitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nothing")
    }
}

impl Display for BooleanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a boolean")
    }
}

impl Display for I32Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "an i32")
    }
}

impl Display for I64Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "an i64")
    }
}

impl Display for DecimalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a decimal")
    }
}

impl Display for StringType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a string")
    }
}

impl Display for CharacterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a character")
    }
}

impl Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a function")
    }
}

impl Display for StructType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "a struct")
    }
}

impl Pseudo for Type {
    fn pseudo(&self) -> String {
        match self {
            Type::Unit(_) => "unit".to_string(),
            Type::Boolean(_) => "bool".to_string(),
            Type::I32(_) => "i32".to_string(),
            Type::I64(_) => "i64".to_string(),
            Type::Decimal(_) => "decimal".to_string(),
            Type::String(_) => "string".to_string(),
            Type::Character(_) => "char".to_string(),
            Type::Function(f) => {
                let mut s = String::from("fn(");
                for (i, param) in f.parameters.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&param.pseudo());
                }
                s.push_str(") -> ");
                s.push_str(&f.returns.pseudo());
                s
            }
            Type::Struct(s) => s
                .name
                .as_ref()
                .map_or("struct".to_string(), |name| name.name.to_string()),
        }
    }
}

impl From<Type> for cranelift::prelude::Type {
    fn from(value: Type) -> Self {
        match value {
            Type::Unit(_) => types::I8,
            Type::Boolean(_) => types::I8,
            Type::I32(_) => types::I32,
            Type::I64(_) => types::I64,
            Type::Decimal(_) => types::F64,
            Type::String(_) => todo!("string cranelift type"),
            Type::Character(_) => todo!("character cranelift type"),
            Type::Function(_) => types::I64, // pointer
            Type::Struct(_) => todo!("struct cranelift type"),
        }
    }
}

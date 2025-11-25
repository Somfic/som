use crate::{
    ast::{Expression, Pseudo, Type},
    lexer::Path,
    parser::Untyped,
    Phase, Result, TypeCheckError,
};
use std::collections::HashMap;

mod expression;
mod module;
pub use module::*;
mod file;
mod statement;

#[derive(Debug)]
pub struct Typed;

impl Phase for Typed {
    type TypeInfo = Type;
}

pub trait TypeCheck: Sized {
    type Output;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output>;
}

pub struct TypeCheckContext<'a> {
    parent: Option<&'a TypeCheckContext<'a>>,
    variables: HashMap<String, Type>,
    types: HashMap<String, Type>,
    registry: &'a HashMap<Path, ModuleScope>,
}

impl<'a> TypeCheckContext<'a> {
    pub fn new(registry: &'a HashMap<Path, ModuleScope>) -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            types: HashMap::new(),
            registry,
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

    pub fn get_type(&self, name: impl Into<String>) -> Result<Type> {
        let name = name.into();

        self.types
            .get(&name)
            .cloned()
            .or_else(|| {
                self.parent
                    .and_then(|parent_ctx| parent_ctx.get_type(name).ok())
            })
            .ok_or_else(|| TypeCheckError::UndefinedType.to_diagnostic())
    }

    pub fn get_type_with_span(&self, name: impl Into<String>, span: &crate::Span) -> Result<Type> {
        let name = name.into();

        self.types
            .get(&name)
            .cloned()
            .or_else(|| {
                self.parent
                    .and_then(|parent_ctx| parent_ctx.get_type(&name).ok())
            })
            .ok_or_else(|| {
                TypeCheckError::UndefinedType
                    .to_diagnostic()
                    .with_label(span.label(format!("type '{}' not found", name)))
            })
    }

    pub fn declare_type(&mut self, name: impl Into<String>, ty: Type) {
        self.types.insert(name.into(), ty);
    }

    fn new_child_context(&self) -> TypeCheckContext<'_> {
        TypeCheckContext {
            parent: Some(self),
            variables: HashMap::new(),
            types: HashMap::new(),
            registry: self.registry,
        }
    }

    pub fn get_module_scope(&self, path: &Path) -> Result<&ModuleScope> {
        self.registry
            .get(path)
            .ok_or_else(|| {
                TypeCheckError::UndefinedModule
                    .to_diagnostic()
                    .with_label(path.span.label(format!("module '{}' not found", path)))
            })
    }
}

pub fn expect_type(a: &Type, b: &Type, hint: impl Into<String>) -> Result<()> {
    if a == b {
        return Ok(());
    }

    Err(TypeCheckError::TypeMismatch
        .to_diagnostic()
        .with_label(a.span().label(format!("{}", a.pseudo())))
        .with_label(b.span().label(format!("{}", b.pseudo())))
        .with_hint(hint.into()))
}

pub fn expect_boolean(actual: &Type, hint: impl Into<String>) -> Result<()> {
    if !matches!(actual, Type::Boolean(_)) {
        return TypeCheckError::ExpectedType
            .to_diagnostic()
            .with_label(
                actual
                    .span()
                    .label(format!("{}, expected a boolean", actual)),
            )
            .with_hint(hint.into())
            .to_err();
    }
    Ok(())
}

pub fn expect_struct(actual: &Type, hint: impl Into<String>) -> Result<()> {
    if !matches!(actual, Type::Struct(_)) {
        return TypeCheckError::ExpectedType
            .to_diagnostic()
            .with_label(
                actual
                    .span()
                    .label(format!("{}, expected a struct", actual)),
            )
            .with_hint(hint.into())
            .to_err();
    }
    Ok(())
}

// TypeCheck implementation for Type to automatically resolve Forward types
impl TypeCheck for Type {
    type Output = Type;

    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self::Output> {
        match self {
            Type::Forward(forward) => {
                // Look up the Forward type in the context to get the resolved type
                ctx.get_type_with_span(forward.name.to_string(), &forward.span)
            }
            // All other types are already resolved, just return them
            other => Ok(other),
        }
    }
}

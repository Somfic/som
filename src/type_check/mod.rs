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
    dispatch_functions: HashMap<String, Vec<DispatchImplementation>>,
    types: HashMap<String, Type>,
    registry: &'a HashMap<Path, ModuleScope>,
}

pub struct DispatchImplementation {
    pub function_id: usize,
    pub parameter_type: Type,
}

impl<'a> TypeCheckContext<'a> {
    pub fn new(registry: &'a HashMap<Path, ModuleScope>) -> Self {
        Self {
            parent: None,
            variables: HashMap::new(),
            types: HashMap::new(),
            dispatch_functions: HashMap::new(),
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

    pub fn declare_dispatch_function(
        &mut self,
        name: impl Into<String>,
        function_id: usize,
        parameter_type: Type,
    ) {
        let name = name.into();

        let implementations = self.dispatch_functions.entry(name).or_default();

        implementations.push(DispatchImplementation {
            function_id,
            parameter_type,
        });
    }

    pub fn get_dispatch_implementations(
        &self,
        name: impl Into<String>,
    ) -> Result<&Vec<DispatchImplementation>> {
        let name = name.into();

        self.dispatch_functions
            .get(&name)
            .or_else(|| {
                self.parent
                    .and_then(|parent_ctx| parent_ctx.get_dispatch_implementations(&name).ok())
            })
            .ok_or_else(|| TypeCheckError::UndefinedFunction.to_diagnostic())
    }

    pub fn find_dispatch_implementation(
        &self,
        name: impl Into<String>,
        parameter_type: &Type,
    ) -> Result<usize> {
        let name = name.into();

        let implementations = self.get_dispatch_implementations(&name)?;

        for implementation in implementations {
            if &implementation.parameter_type == parameter_type {
                return Ok(implementation.function_id);
            }
        }

        TypeCheckError::UndefinedFunction
            .to_diagnostic()
            .with_hint(format!(
                "no implementation of '{}' for type '{}'",
                name, parameter_type
            ))
            .to_err()
    }

    fn new_child_context(&self) -> TypeCheckContext<'_> {
        TypeCheckContext {
            parent: Some(self),
            variables: HashMap::new(),
            types: HashMap::new(),
            dispatch_functions: HashMap::new(),
            registry: self.registry,
        }
    }

    pub fn get_module_scope(&self, path: &Path) -> Result<&ModuleScope> {
        self.registry.get(path).ok_or_else(|| {
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

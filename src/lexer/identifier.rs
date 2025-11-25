use std::{
    fmt::Display,
    hash::{Hash, Hasher},
};

use cranelift::prelude::{types, InstBuilder, Value};

use crate::{
    lexer::TokenKind, Emit, FunctionContext, ModuleContext, Parse, ParserError, Result, Span,
};

#[derive(Debug, Clone)]
pub struct Identifier {
    pub name: Box<str>,
    pub span: Span,
}

impl From<Identifier> for String {
    fn from(val: Identifier) -> Self {
        val.name.to_string()
    }
}

impl Parse for Identifier {
    type Params = ();

    fn parse(input: &mut crate::Parser, params: Self::Params) -> Result<Self> {
        let name = input.expect(
            TokenKind::Identifier,
            "variable name",
            ParserError::ExpectedIdentifier,
        )?;

        match name.value {
            crate::lexer::TokenValue::Identifier(ident) => Ok(ident),
            _ => unreachable!(),
        }
    }
}

impl Emit for Identifier {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        if let Some((func_id, _sig)) = ctx.extern_registry.get(&*self.name) {
            // It's an extern function - get a reference
            let func_ref = ctx.module.declare_func_in_func(*func_id, ctx.builder.func);
            let address = ctx.builder.ins().func_addr(types::I64, func_ref);
            return Ok(address);
        }

        // Check if this identifier refers to a self-referencing lambda
        if let Some(&lambda_id) = ctx.self_referencing_lambdas.get(&*self.name) {
            // This is a recursive call - emit function address
            let (func_id, _) = ctx
                .lambda_registry
                .get(lambda_id)
                .ok_or_else(|| crate::EmitError::UndefinedFunction.to_diagnostic())?;
            let reference = ctx.module.declare_func_in_func(*func_id, ctx.builder.func);
            return Ok(ctx.builder.ins().func_addr(types::I64, reference));
        }

        // Check if this identifier refers to a global function
        if let Some(&lambda_id) = ctx.global_functions.get(&*self.name) {
            // This is a top-level function reference - emit function address
            let (func_id, _) = ctx
                .lambda_registry
                .get(lambda_id)
                .ok_or_else(|| crate::EmitError::UndefinedFunction.to_diagnostic())?;
            let reference = ctx.module.declare_func_in_func(*func_id, ctx.builder.func);
            return Ok(ctx.builder.ins().func_addr(types::I64, reference));
        }

        // Regular variable lookup
        Ok(ctx.builder.use_var(ctx.get_variable(&self.name)?))
    }
}

impl Identifier {
    pub fn new(name: impl Into<Box<str>>, span: Span) -> Self {
        Self {
            name: name.into(),
            span,
        }
    }
}

impl From<&Identifier> for String {
    fn from(value: &Identifier) -> Self {
        value.name.to_string()
    }
}

impl From<Identifier> for Span {
    fn from(identifier: Identifier) -> Self {
        identifier.span
    }
}

impl From<&Identifier> for Span {
    fn from(identifier: &Identifier) -> Self {
        identifier.span.clone()
    }
}

impl Hash for Identifier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq for Identifier {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

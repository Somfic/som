use crate::parser::ast::{Expression, ExpressionValue, Primitive};
use miette::Result;
use std::collections::HashMap;

pub mod error;
use error::*;

#[derive(Debug, Clone, Copy)]
pub struct Value;
pub struct Use;

pub struct TypeChecker {}

impl TypeChecker {
    pub fn var(&mut self) -> (Value, Use) {
        todo!()
    }

    pub fn bool(&mut self) -> Value {
        todo!()
    }
    pub fn bool_use(&mut self) -> Use {
        todo!()
    }

    pub fn func(&mut self, arg: Use, ret: Value) -> Value {
        todo!()
    }
    pub fn func_use(&mut self, arg: Value, ret: Use) -> Use {
        todo!()
    }

    pub fn obj(&mut self, fields: Vec<(String, Value)>) -> Value {
        todo!()
    }
    pub fn obj_use(&mut self, field: (String, Use)) -> Use {
        todo!()
    }

    pub fn case(&mut self, case: (String, Value)) -> Value {
        todo!()
    }
    pub fn case_use(&mut self, cases: Vec<(String, Use)>) -> Use {
        todo!()
    }

    pub fn flow(&mut self, lhs: Value, rhs: Use) -> Result<(), TypeError> {
        todo!()
    }
}

struct Bindings {
    m: HashMap<String, Value>,
}

impl Bindings {
    fn new() -> Self {
        Self { m: HashMap::new() }
    }

    fn get(&self, k: &str) -> Option<Value> {
        self.m.get(k).copied()
    }

    fn insert(&mut self, k: String, v: Value) {
        self.m.insert(k.clone(), v);
    }

    fn in_child_scope<T>(&mut self, callback: impl FnOnce(&mut Self) -> T) -> T {
        let mut child_scope = Bindings { m: self.m.clone() };
        callback(&mut child_scope)
    }
}

fn check_expression<'de>(
    checker: &mut TypeChecker,
    bindings: &mut Bindings,
    expression: Expression<'de>,
) -> Result<Value, Error<'de>> {
    match &expression.value {
        ExpressionValue::Primitive(primitive) => match &primitive {
            Primitive::Boolean(_) => Ok(checker.bool()),
            Primitive::Identifier(ident) => match bindings.get(&ident) {
                Some(value) => Ok(value),
                None => Err(Error::TypeError(TypeError::UnknownIdentifier {
                    identifier: ident.clone(),
                    span: expression.span,
                })),
            },
            _ => todo!(),
        },
        _ => todo!(),
    }
}

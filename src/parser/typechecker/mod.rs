use super::ast::{typed, untyped};
use miette::Result;

pub struct TypeChecker<'de> {
    symbol: untyped::Symbol<'de>,
}

impl<'de> TypeChecker<'de> {
    pub fn new(symbol: untyped::Symbol<'de>) -> Self {
        Self { symbol }
    }

    pub fn check(&mut self) -> Result<typed::Symbol<'de>> {
        Ok(self.symbol.clone())
    }
}

use crate::prelude::*;

pub struct Runner {}

impl Runner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(&self, pointer: *const u8) -> ParserResult<i64> {
        let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(pointer) };
        let result = compiled_fn();
        Ok(result)
    }
}

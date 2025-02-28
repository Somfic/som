use crate::prelude::*;
use cranelift::codegen::CompiledCode;
use memmap2::MmapMut;

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

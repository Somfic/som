use crate::prelude::*;
use cranelift::codegen::CompiledCode;
use memmap2::MmapMut;

pub struct Runner {
    compiled: CompiledCode,
}

impl Runner {
    pub fn new(compiled: CompiledCode) -> Self {
        Self { compiled }
    }

    pub fn run(&self) -> Result<i64> {
        let code_bytes = self.compiled.code_buffer();
        let code_size = code_bytes.len();
        let mut mmap = MmapMut::map_anon(code_size).expect("failed to create mmap");
        mmap[..].copy_from_slice(code_bytes);
        let exec_mmap = mmap.make_exec().expect("failed to make mmap executable");
        let code_ptr = exec_mmap.as_ptr();

        // cast the code pointer to a function pointer; our function takes no arguments and returns i64
        let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(code_ptr) };

        // call the JITted function and print the result
        let result = compiled_fn();

        Ok(result)
    }
}

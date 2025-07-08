use crate::prelude::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

pub struct Runner {}

impl Runner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(self, compiled: *const u8) -> Result<i64> {
        let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(compiled) };

        #[allow(clippy::redundant_closure)]
        let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));

        match result {
            Ok(value) => Ok(value),
            Err(_) => panic!("An error occurred while running the compiled function."),
        }
    }
}

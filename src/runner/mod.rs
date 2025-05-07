use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{
    ast::TypedModule,
    compiler::{self, Compiler},
    parser::{self, Parser},
    prelude::*,
    runner,
    tokenizer::Tokenizer,
    typer::{self, Typer},
};

pub struct Runner {
    compiled: *const u8,
}

impl Runner {
    pub fn new(compiled: *const u8) -> Self {
        Self { compiled }
    }

    pub fn run(self) -> ReportResult<i64> {
        let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(self.compiled) };
        let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));
        match result {
            Ok(value) => Ok(value),
            Err(_) => Err(vec![miette::miette!("Runtime error")]),
        }
    }
}

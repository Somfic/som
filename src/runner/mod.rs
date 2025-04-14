use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{ast::TypedModule, compiler, parser, prelude::*, runner, typer};

pub struct Runner {}

impl Runner {
    pub fn new() -> Self {
        Self {}
    }

    pub fn run(&self, pointer: *const u8) -> ParserResult<i64> {
        let compiled_fn: extern "C" fn() -> i64 = unsafe { std::mem::transmute(pointer) };
        let result = catch_unwind(AssertUnwindSafe(|| compiled_fn()));
        match result {
            Ok(value) => Ok(value),
            Err(_) => Err(vec![miette::diagnostic!("Runtime error")]),
        }
    }
}

pub fn run(source_code: impl Into<String>) -> i64 {
    let source_code = source_code.into();

    let modules = parse(&source_code)
        .map_err(|errors| {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source_code.clone())
                );
            }
        })
        .expect("failed to parse expression");

    let compiled = compile(modules)
        .map_err(|error| {
            for error in error {
                eprintln!("{:?}", error);
            }
        })
        .expect("failed to compile expression");

    runner::Runner::new()
        .run(compiled)
        .map_err(|error| {
            for error in error {
                eprintln!("{:?}", error);
            }
        })
        .expect("failed to run expression")
}

fn parse<'ast>(source_code: impl Into<String>) -> ParserResult<Vec<TypedModule<'ast>>> {
    let source_code = source_code.into();
    let modules = parser::Parser::new(Box::leak(source_code.into_boxed_str())).parse()?;
    typer::Typer::new().type_check(modules)
}

fn compile(modules: Vec<TypedModule<'_>>) -> CompilerResult<*const u8> {
    compiler::Compiler::new().compile(modules)
}

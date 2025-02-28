use cranelift::codegen::CompiledCode;

use crate::{
    ast::{TypedExpression, TypedModule, TypedStatement},
    compiler::{self},
    parser::{self},
    run,
    runner::{self},
    typer::{self},
    CompilerResult, ParserResult,
};

mod binary;
mod block;
mod conditional;
mod group;
mod unary;
mod variables;

pub fn run_and_assert(source_code: impl Into<String>, expected: i64) {
    assert_eq!(run(source_code), expected);
}

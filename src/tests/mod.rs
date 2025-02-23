use cranelift::codegen::CompiledCode;

use crate::{
    ast::TypedExpression,
    compiler::{self, Compiler},
    parser::{self, Parser},
    runner::{self, Runner},
    typer::{self, Typer},
    CompilerResult, ParserResult,
};

mod binary;
mod block;
mod conditional;
mod group;
mod unary;

pub fn run_and_assert(source_code: impl Into<String>, expected: i64) {
    let source_code = source_code.into();

    println!("{}\n", source_code);

    let expression = parse(&source_code)
        .map_err(|errors| {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source_code.clone())
                );
            }
        })
        .expect("failed to parse expression");

    let compiled = compile(expression)
        .map_err(|error| {
            for error in error {
                eprintln!("{:?}", error);
            }
        })
        .expect("failed to compile expression");

    let result = runner::Runner::new(compiled)
        .run()
        .expect("failed to run expression");

    assert_eq!(result, expected);
}

fn parse(source_code: impl Into<String>) -> ParserResult<TypedExpression<'static>> {
    let source_code = source_code.into();
    let expression = parser::Parser::new(Box::leak(source_code.into_boxed_str())).parse()?;
    typer::Typer::new(expression).type_check()
}

fn compile(expression: TypedExpression<'_>) -> CompilerResult<CompiledCode> {
    compiler::Compiler::new(expression).compile()
}

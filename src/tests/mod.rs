use cranelift::codegen::CompiledCode;

use crate::{
    ast::{TypedExpression, TypedStatement},
    compiler::{self},
    parser::{self},
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

fn parse<'ast>(source_code: impl Into<String>) -> ParserResult<Vec<TypedStatement<'ast>>> {
    let source_code = source_code.into();
    let statements = parser::Parser::new(Box::leak(source_code.into_boxed_str())).parse()?;
    typer::Typer::new().type_check(statements)
}

fn compile<'ast>(statements: Vec<TypedStatement<'ast>>) -> CompilerResult<CompiledCode> {
    compiler::Compiler::new().compile(statements)
}

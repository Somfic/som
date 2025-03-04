mod prelude;
use ast::TypedModule;
use highlighter::SomHighlighter;
pub use prelude::*;

mod ast;
mod compiler;
mod highlighter;
mod parser;
mod runner;
#[cfg(test)]
mod tests;
mod tokenizer;
mod typer;

const INPUT: &str = "
fn main() { 
    let dawd = a;
    let a = 0;
}

fn main() { 
   0
}
";

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let result = run(INPUT);
    println!("Result: {}", result);
}

pub fn run(source_code: impl Into<String>) -> i64 {
    let source_code = source_code.into();

    println!("{}\n", source_code);

    let statements = parse(&source_code)
        .map_err(|errors| {
            errors.print(source_code);
        })
        .expect("failed to parse expression");

    let compiled = compile(statements)
        .map_err(|error| {
            for error in error {
                eprintln!("{:?}", error);
            }
        })
        .expect("failed to compile expression");

    runner::Runner::new()
        .run(compiled)
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

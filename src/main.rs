use crate::prelude::*;
use parser::Parser;

mod expressions;
mod lexer;
mod parser;
mod prelude;
mod statements;
mod type_checker;
mod types;

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .with_cause_chain()
                .context_lines(2)
                .build(),
        )
    }))
    .unwrap();

    let source = "
    { 
        type Color = { r ~ int, g ~ int, b ~ int };
        let red: Color = { r: 255, g: 0, b: 0 };

        let a ~ int = red.r + red.g + red.b;
    };";

    let lexer = Lexer::new(source);

    let mut parser = Parser::new(lexer);
    let parsed = match parser.parse() {
        Ok(parsed) => parsed,
        Err(errors) => {
            for error in errors {
                eprintln!("{:?}", miette::miette!(error).with_source_code(source));
            }
            std::process::exit(1);
        }
    };

    let mut type_checker = TypeChecker::new();
    let type_checked = match type_checker.check(&parsed) {
        Ok(typed_statement) => typed_statement,
        Err(errors) => {
            for error in errors {
                eprintln!("{:?}", miette::miette!(error).with_source_code(source));
            }
            std::process::exit(1);
        }
    };
}

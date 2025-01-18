use crate::typer::TypeChecker;
use highlighter::SomHighlighter;
use lexer::Lexer;
use miette::miette;
use parser::Parser;
use std::vec;

pub mod highlighter;
pub mod lexer;
pub mod parser;
pub mod typer;

const INPUT: &str = "

fn main() {
    let a = 1 + '2';
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

    let mut errors = vec![];

    let lexer = Lexer::new(INPUT);

    let mut parser = Parser::new(lexer);
    let statements = match parser.parse() {
        Ok(statements) => statements,
        Err(err) => {
            println!("{:?}", err.with_source_code(INPUT));
            return;
        }
    };

    let typechecker = TypeChecker::new(&statements);
    errors.extend(typechecker.check());

    for error in errors {
        println!("{:?}", miette!(error).with_source_code(INPUT));
    }
}

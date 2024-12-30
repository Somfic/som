use crate::parser::typechecker::TypeChecker;
use highlighter::SomHighlighter;
use lexer::{Lexer, TokenKind};
use miette::{miette, Diagnostic};
use owo_colors::{Style, Styled};
use parser::{
    ast::untyped::{Expression, ExpressionValue},
    Parser,
};
use std::vec;
use thiserror::Error;

pub mod highlighter;
pub mod lexer;
pub mod parser;

const INPUT: &str = "
fn add(left ~ int, right ~ int) -> fn(int, int) -> int {
    left + right
}

fn main() {
    let a = 1;
    let b = 2;
    let c = add(a, b);
}
";

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(3)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let mut errors = vec![];

    let lexer = Lexer::new(INPUT);

    let mut parser = Parser::new(lexer);
    let symbol = match parser.parse() {
        Ok(symbol) => symbol,
        Err(err) => {
            println!("{:?}", err.with_source_code(INPUT));
            return;
        }
    };

    let typechecker = TypeChecker::new(symbol);
    errors.extend(typechecker.check());

    for error in errors {
        println!("{:?}", miette!(error).with_source_code(INPUT));
    }
}

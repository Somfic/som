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
enum Test: a, b, c;

fn main() {
    let a = 12;
    let b = 'a';
    let c = a + b;
    let c = a + b;
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

use crate::prelude::*;
use miette::Context;
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
                .context_lines(2)
                .build(),
        )
    }))
    .unwrap();

    let source = "1 + true;";

    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);

    let statement = parser
        .parse_statement(true)
        .context("Failed to parse statement")
        .unwrap();

    let mut type_checker = TypeChecker::new();
    let type_checked = type_checker.check(&statement);
}

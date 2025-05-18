use crate::prelude::*;
use miette::Context;
use parser::Parser;

mod expressions;
mod lexer;
mod parser;
mod prelude;
mod statements;
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

    let source = "1 + 1";

    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);

    parser
        .parse_expression(BindingPower::None)
        .context("while parsing")
        .map_err(|e| {
            eprintln!("{:?}", miette::miette!(e).with_source_code(source));
        })
        .ok();
}

use lexer::{Lexer, TokenKind};
use miette::Context;

mod lexer;
mod prelude;

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

    let source = "1.1 1,";

    let mut lexer = Lexer::new(source);

    lexer
        .expect(TokenKind::Arrow)
        .context("while parsing")
        .map_err(|e| {
            eprintln!("{:?}", miette::miette!(e).with_source_code(source));
        })
        .ok();
}

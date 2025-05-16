use lexer::Lexer;

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

    let lexer = Lexer::new(source);

    for token in lexer {
        match token {
            Ok(tok) => println!("{tok:?}"),
            Err(e) => {
                eprintln!("{:?}", miette::miette!(e).with_source_code(source));
                break;
            }
        }
    }
}

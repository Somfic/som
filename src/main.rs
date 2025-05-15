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

    let mut lexer = Lexer::new("123");

    match lexer.next_token() {
        Ok(token) => println!("Next token: {:?}", token),
        Err(errors) => {
            for error in errors {
                eprintln!("{:?}", miette::miette!(error).with_source_code("123"));
            }
        }
    }
}

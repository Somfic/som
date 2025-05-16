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

    let source = "123_~`|@#%$^&*()_+-=[]{};':\",.<>?/\\\n\n\nlet x = 42;\nfn main() {\n    println!(\"Hello, world!\");\n}\n";

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

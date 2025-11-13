use som::{ast::Expression, Diagnostic, Parser, Source};

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
    }
}

fn run() -> Result<(), Diagnostic> {
    let source = Source::from_raw("true+truee");
    let mut parser = Parser::new(source);
    let code: Expression<_> = parser.parse()?;

    println!("{:?}", code);
    Ok(())
}

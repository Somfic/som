use som::{ast::Expression, Parser, Source};

fn main() -> Result<(), String> {
    let source = Source::from_raw("true+true");
    let mut parser = Parser::new(source);
    let code: Expression<_> = parser.parse().map_err(|e| format!("{:?}", e))?;

    println!("{:?}", code);
    Ok(())
}

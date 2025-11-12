use som::{ast::Expression, Parser, Source};

fn main() -> Result<(), String> {
    let source = Source::Raw("true+true");
    let mut parser = Parser::new(source);
    let code = parser
        .parse::<Expression>()
        .map_err(|e| format!("{:?}", e))?;

    println!("{:?}", code);
    Ok(())
}

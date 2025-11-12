use som::{ast::Expr, Parser, Source};

fn main() -> Result<(), String> {
    let source = Source::Raw("true+true");
    let mut parser = Parser::new(source);
    let code = parser.parse::<Expr>().map_err(|e| format!("{:?}", e))?;
    println!("{:?}", code);
    Ok(())
}

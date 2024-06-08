use anyhow::*;

pub mod parser;
pub mod scanner;

fn main() -> Result<()> {
    let code = "'1' * 10 + 1 + 1 - 1;";
    let tokens = scanner::Scanner::new(code.to_owned()).collect::<Vec<_>>();
    println!("{:#?}", tokens);

    let mut parser = parser::Parser::new(tokens);
    let parsed = parser.parse();
    println!("{:#?}", parsed);

    Ok(())
}

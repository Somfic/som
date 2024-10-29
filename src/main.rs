use lexer::Lexer;
use miette::LabeledSpan;
use parser::Parser;

pub mod lexer;
pub mod parser;

fn main() {
    let input = "let value = if 12 % 2 == 0 true else false;\n";

    let mut parser = Parser::new(input);
    let symbol = match parser.parse() {
        Ok(symbol) => symbol,
        Err(err) => {
            println!("{:?}", err);
            return;
        }
    };

    println!("{:?}", symbol);
}

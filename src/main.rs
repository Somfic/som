use lexer::Lexer;
use miette::LabeledSpan;
use parser::Parser;

pub mod lexer;
pub mod parser;

fn main() {
    let input = "1";

    let mut lexer = Lexer::new(input);
    let tokens = lexer.collect::<Vec<_>>();

    for token in &tokens {
        println!("{:?}", token);
    }

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

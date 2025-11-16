use som::{lexer::Lexer, Source};

fn main() {
    let source = Source::from_raw("true+true");
    let mut lexer = Lexer::new(source);

    println!("Tokens:");
    while let Some(result) = lexer.next() {
        match result {
            Ok(token) => println!("  {:?}", token),
            Err(e) => println!("  Error: {:?}", e),
        }
    }
}

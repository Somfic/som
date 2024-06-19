use anyhow::Result;
use ast::builder::build_ast;
use core::result::Result::Ok;
use files::Files;

pub mod ast;
pub mod diagnostic;
pub mod files;
pub mod parser;
pub mod scanner;

fn main() -> Result<()> {
    let mut files = Files::default();
    files.insert(
        "main",
        "
        enum colors: green blue red;
    ",
    );

    let scanner = scanner::Scanner::new(&files);
    let tokens = scanner.parse();

    let tokens = match &tokens {
        Ok(tokens) => tokens,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to scan");
        }
    };

    let parser = parser::EarleyParser::default();

    let parse_tree = match parser.parse(tokens) {
        Ok(parse_tree) => parse_tree,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to parse");
        }
    };

    println!("{:#?}", parse_tree);

    let ast = match build_ast(&parse_tree) {
        Ok(ast) => ast,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to build AST");
        }
    };

    Ok(())
}

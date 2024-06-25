use abstract_syntax_tree::builder::build_ast;
use anyhow::Result;
use core::result::Result::Ok;
use files::Files;

pub mod abstract_syntax_tree;
pub mod concrete_syntax_tree;
pub mod diagnostic;
pub mod files;
pub mod scanner;

fn main() -> Result<()> {
    let mut files = Files::default();
    files.insert(
        "main",
        "
        enum colors: green blue red

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

    let parser = concrete_syntax_tree::EarleyParser::default();

    let concrete_syntax = match parser.parse(tokens) {
        Ok(parse_tree) => parse_tree,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to parse");
        }
    };

    println!("Concrete syntax:\n{:#?}", concrete_syntax);

    let ast = match build_ast(&concrete_syntax) {
        Ok(ast) => ast,
        Err(diagnostics) => {
            diagnostics
                .iter()
                .for_each(|diagnostic| diagnostic.print(&files));
            panic!("Failed to build AST");
        }
    };

    println!("Abstract syntax:\n{:#?}", ast);

    Ok(())
}

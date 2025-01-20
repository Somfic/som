use crate::typer::TypeChecker;
use highlighter::SomHighlighter;
use lexer::Lexer;
use miette::miette;
use parser::Parser;
use std::vec;

pub mod ast;
pub mod compiler;
pub mod highlighter;
pub mod lexer;
pub mod parser;
pub mod typer;

const INPUT: &str = "
fn main() {
    let a = 1;
}
";

pub type Result<T> = std::result::Result<T, Vec<miette::MietteDiagnostic>>;

fn main() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let mut errors = vec![];

    let lexer = Lexer::new(INPUT);

    let mut parser = Parser::new(lexer);
    let module: ast::Module<'_, ast::Expression<'_>> = match parser.parse() {
        Ok(statements) => statements,
        Err(err) => {
            println!("{:?}", err.with_source_code(INPUT));
            return;
        }
    };

    let mut typechecker = TypeChecker::new();
    match typechecker.type_check(vec![module]) {
        Ok(modules) => {
            println!("{:?}", modules);
        }
        Err(err) => errors.extend(err),
    }

    for error in errors {
        println!("{:?}", miette!(error).with_source_code(INPUT));
    }
}

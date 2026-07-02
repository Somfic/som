use som_common::{DiagnosticSink, Id, Source};

mod ast;
mod lexer;
mod parser;
mod pretty;
mod token;

pub use ast::*;
pub use lexer::lex;
pub use parser::Parser;
pub use pretty::AstCtx;
pub use token::*;

pub fn parse(source: Id<Source>, content: &str, diags: &mut DiagnosticSink) -> Ast {
    let tokens = lex(source, content);

    let parser = Parser::new(tokens, diags);
    parser.parse()
}

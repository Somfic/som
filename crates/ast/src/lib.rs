use som::*;

mod lexer;
mod parser;
mod token;

use lexer::*;
pub use parser::Parser;
use token::*;

#[derive(Debug, Default)]
pub struct Ast {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    pub root: Vec<Id<Stmt>>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            root: Vec::new(),
        }
    }

    pub fn add_expr(&mut self, expr: Expr) -> Id<Expr> {
        self.exprs.alloc(expr)
    }

    pub fn add_stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.stmts.alloc(stmt)
    }

    pub fn get_expr(&self, id: Id<Expr>) -> &Expr {
        self.exprs.get(&id)
    }

    pub fn get_stmt(&self, id: Id<Stmt>) -> &Stmt {
        self.stmts.get(&id)
    }
}

expand_enum! {
#[derive(Debug)]
pub enum Expr {
    Error,
    Int { value: i64 },
    Binary { lhs: Id<Expr>, op: TokenKind, rhs: Id<Expr> },
} with {
    span: Span,
}}

expand_enum! {
#[derive(Debug)]
pub enum Stmt {
    Expr { expr: Id<Expr> },
} with {
    span: Span,
}}

pub fn parse(source: Id<Source>, content: &str, diags: &mut DiagnosticSink) -> Ast {
    let tokens = lex(source, content);
    if std::env::var("SOM_DUMP_TOKENS").is_ok() {
        info!("tokens dump:\n{tokens:#?}");
    }

    let parser = Parser::new(tokens, diags);
    let ast = parser.parse();

    ast
}

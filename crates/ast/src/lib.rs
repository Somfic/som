use std::ops::Index;

use som_common::*;

mod lexer;
mod parser;
mod token;

use lexer::*;
pub use parser::Parser;
pub use token::*;

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
}

impl Index<Id<Expr>> for Ast {
    type Output = Expr;
    fn index(&self, id: Id<Expr>) -> &Expr {
        self.exprs.get(&id)
    }
}

impl Index<Id<Stmt>> for Ast {
    type Output = Stmt;
    fn index(&self, id: Id<Stmt>) -> &Stmt {
        self.stmts.get(&id)
    }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Expr {
        Error,
        Int { value: i64 },
        Binary { lhs: Id<Expr>, op: BinaryOp, rhs: Id<Expr> },
    } with { span: Span }
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
        };
        f.write_str(s)
    }
}

#[derive(Copy, Clone)]
pub struct AstCtx<'a> {
    pub sources: Option<&'a SourceMap>,
}

impl Ast {
    pub fn display<'a>(&'a self) -> Show<'a, Ast, AstCtx<'a>> {
        Show::new(self, AstCtx { sources: None })
    }

    pub fn display_with_sources<'a>(&'a self, sources: &'a SourceMap) -> Show<'a, Ast, AstCtx<'a>> {
        Show::new(
            self,
            AstCtx {
                sources: Some(sources),
            },
        )
    }
}

impl Pretty<AstCtx<'_>> for Ast {
    fn pretty(&self, ctx: AstCtx<'_>, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut w = LineWriter::new(f, ctx.sources);
        for stmt_id in &self.root {
            let stmt = &self[*stmt_id];
            let span = stmt.span();
            let mut buf = String::new();
            fmt_stmt(&mut buf, self, *stmt_id);
            buf.push(';');
            w.line(Some(span), 0, buf)?;
        }
        Ok(())
    }
}

fn fmt_stmt(buf: &mut String, ast: &Ast, id: Id<Stmt>) {
    match ast[id] {
        Stmt::Expr { expr, .. } => fmt_expr(buf, ast, expr, false),
    }
}

fn fmt_expr(buf: &mut String, ast: &Ast, id: Id<Expr>, nested: bool) {
    use std::fmt::Write;
    match ast[id] {
        Expr::Error { .. } => {
            let _ = buf.write_str("<error>");
        }
        Expr::Int { value, .. } => {
            let _ = write!(buf, "{value}");
        }
        Expr::Binary { lhs, op, rhs, .. } => {
            if nested {
                buf.push('(');
            }
            fmt_expr(buf, ast, lhs, true);
            let _ = write!(buf, " {op} ");
            fmt_expr(buf, ast, rhs, true);
            if nested {
                buf.push(')');
            }
        }
    }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Stmt {
        Expr { expr: Id<Expr> },
    } with { span: Span }
}

pub fn parse(source: Id<Source>, content: &str, diags: &mut DiagnosticSink) -> Ast {
    let tokens = lex(source, content);

    let parser = Parser::new(tokens, diags);
    parser.parse()
}

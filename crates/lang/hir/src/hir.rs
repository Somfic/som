use som_ast::{BinaryOp, UnaryOp};
use som_common::{Arena, Id, Span, expand_enum};

use crate::Type;

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Expr {
        Error,
        Int { value: i64 },
        Bool { value: bool },
        Unary { op: UnaryOp, operand: Id<Expr> },
        Binary { lhs: Id<Expr>, op: BinaryOp, rhs: Id<Expr> },
        Condition { condition: Id<Expr>, truthy: Id<Expr>, falsy: Id<Expr> },
        Block { stmts: Vec<Id<Stmt>>, value: Option<Id<Expr>> },
        Assignment { target: Box<str>, binding: Option<Id<Binding>>, value: Id<Expr> },
        Variable { name: Box<str>, binding: Option<Id<Binding>> },
    } with { span: Span, ty: Id<Type> }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Stmt {
        Error,
        Expr { expr: Id<Expr> },
        Let { ident: Box<str>, binding: Id<Binding>, expr: Id<Expr> },
    } with { span: Span }
}

#[derive(Debug)]
pub struct Binding {
    pub name: Box<str>,
    pub span: Span,
    pub ty: Id<Type>,
}

#[derive(Debug)]
pub struct Hir {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    bindings: Arena<Binding>,
    pub root: Vec<Id<Stmt>>,
}

impl Hir {
    pub(crate) fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            bindings: Arena::new(),
            root: Vec::new(),
        }
    }

    pub(crate) fn add_expr(&mut self, expr: Expr) -> Id<Expr> {
        self.exprs.alloc(expr)
    }

    pub(crate) fn add_stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.stmts.alloc(stmt)
    }

    pub(crate) fn add_binding(&mut self, binding: Binding) -> Id<Binding> {
        self.bindings.alloc(binding)
    }

    pub fn get_expr(&self, id: Id<Expr>) -> &Expr {
        self.exprs.get(&id)
    }

    pub fn get_stmt(&self, id: Id<Stmt>) -> &Stmt {
        self.stmts.get(&id)
    }

    pub fn binding(&self, id: Id<Binding>) -> &Binding {
        self.bindings.get(&id)
    }
}

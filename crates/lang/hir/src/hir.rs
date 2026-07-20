use std::collections::BTreeMap;

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
        Str { value: Box<str> },
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

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Layout {
        Element {
            tag: Box<str>,
            events: BTreeMap<Box<str>, Id<Expr>>,
            attr: BTreeMap<Box<str>, Id<Expr>>,
            children: Vec<Id<Layout>>
        },
        Text { text: Vec<TextPart> }
    } with { span: Span }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum TextPart {
        Str { text: Box<str> },
        Interp { value: Id<Expr> }
    } with { span: Span }
}

#[derive(Debug)]
pub enum Root {
    Stmt(Id<Stmt>),
    Layout(Id<Layout>),
}

#[derive(Debug)]
pub struct Binding {
    pub name: Box<str>,
    pub span: Span,
    pub ty: Id<Type>,
    /// Whether the binding is ever assigned to. Set by the typer when it sees an
    /// assignment targeting it. Drives the walker's signal-vs-derived choice:
    /// mutable state is a `signal`, an immutable binding is a `derived`.
    pub mutable: bool,
}

#[derive(Debug)]
pub struct Hir {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    layout: Arena<Layout>,
    bindings: Arena<Binding>,
    pub root: Vec<Root>,
}

impl Hir {
    pub(crate) fn new() -> Self {
        Self {
            exprs: Arena::new(),
            stmts: Arena::new(),
            layout: Arena::new(),
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

    pub(crate) fn add_layout(&mut self, layout: Layout) -> Id<Layout> {
        self.layout.alloc(layout)
    }

    pub(crate) fn add_binding(&mut self, binding: Binding) -> Id<Binding> {
        self.bindings.alloc(binding)
    }

    pub fn get_layout(&self, id: Id<Layout>) -> &Layout {
        self.layout.get(&id)
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

    pub(crate) fn binding_mut(&mut self, id: Id<Binding>) -> &mut Binding {
        self.bindings.get_mut(&id)
    }
}

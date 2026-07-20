use som_common::{Arena, Id, Span, expand_enum};
use std::{collections::BTreeMap, ops::Index};

#[derive(Debug, Default)]
pub struct Ast {
    layout: Arena<Layout>,
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    pub root: Vec<Root>,
}

impl Ast {
    pub fn new() -> Self {
        Self {
            layout: Arena::new(),
            exprs: Arena::new(),
            stmts: Arena::new(),
            root: Vec::new(),
        }
    }

    pub fn add_layout(&mut self, layout: Layout) -> Id<Layout> {
        self.layout.alloc(layout)
    }

    pub fn add_expr(&mut self, expr: Expr) -> Id<Expr> {
        self.exprs.alloc(expr)
    }

    pub fn add_stmt(&mut self, stmt: Stmt) -> Id<Stmt> {
        self.stmts.alloc(stmt)
    }
}

impl Index<Id<Layout>> for Ast {
    type Output = Layout;
    fn index(&self, id: Id<Layout>) -> &Layout {
        self.layout.get(&id)
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
    #[derive(Debug, Clone)]
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
    #[derive(Debug, Clone)]
    pub enum TextPart {
        Str { text: Box<str> },
        Interp { value: Id<Expr> }
    } with { span: Span }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone)]
    pub enum Expr {
        Error,
        Int { value: i64 },
        Bool { value: bool },
        Str { value: Box<str> },
        Unary { op: UnaryOp, operand: Id<Expr> },
        Binary { lhs: Id<Expr>, op: BinaryOp, rhs: Id<Expr> },
        Condition { condition: Id<Expr>, truthy: Id<Expr>, falsy: Id<Expr> },
        Block { stmts: Vec<Id<Stmt>>, value: Option<Id<Expr>> },
        Assignment { target: Box<str>, value: Id<Expr> },
        Variable { name: Box<str> }
    } with { span: Span }
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone)]
    pub enum Stmt {
        Expr { expr: Id<Expr> },
        Let { ident: Box<str>, expr: Id<Expr>, ty: Option<Ty> },
    } with { span: Span }
}

#[derive(Debug)]
pub enum Root {
    Stmt(Id<Stmt>),
    Layout(Id<Layout>),
}

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug, Clone, Copy)]
    pub enum Ty {
        Error,
        I32,
        Bool,
    } with { span: Span }
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negate,
    Not,
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            UnaryOp::Negate => "-",
            UnaryOp::Not => "!",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEquals,
    GreaterThan,
    GreaterThanOrEquals,
    And,
    Or,
}

impl std::fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Equals => "==",
            BinaryOp::NotEquals => "!=",
            BinaryOp::LessThan => "<",
            BinaryOp::LessThanOrEquals => "<=",
            BinaryOp::GreaterThan => ">",
            BinaryOp::GreaterThanOrEquals => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        };
        f.write_str(s)
    }
}

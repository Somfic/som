use som_common::{Arena, Id, Span, expand_enum};
use som_hir::{BinaryOp, Type, UnaryOp};

#[rustfmt::skip]
expand_enum! {
    #[derive(Debug)]
    pub enum Statement {
        Assign { local: Id<LocalDecl>, rvalue: Rvalue },
    } with { span: Span }
}

#[derive(Debug)]
pub enum Rvalue {
    Use(Operand),
    UnaryOp(UnaryOp, Operand),
    BinaryOp(Operand, BinaryOp, Operand),
}

#[derive(Debug, Clone)]
pub enum Operand {
    Copy(Id<LocalDecl>),
    Const(Const),
}

#[derive(Debug, Clone)]
pub enum Const {
    Int(i64, Id<Type>),
    Bool(bool, Id<Type>),
}

#[derive(Debug)]
pub enum Terminator {
    Goto(Id<Block>),
    SwitchInt {
        discr: Operand,
        targets: Vec<(i64, Id<Block>)>,
    },
    Return,
    Unreachable,
}

#[derive(Debug)]
pub struct LocalDecl {
    pub ty: Id<Type>,
    pub span: Span,
    pub name: &'static str,
}

#[derive(Debug)]
pub struct Block {
    pub stmts: Vec<Id<Statement>>,
    pub terminator: Terminator,
    pub name: &'static str,
}

#[derive(Debug)]
pub struct Function {
    pub locals: Arena<LocalDecl>,
    pub blocks: Arena<Block>,
    pub statements: Arena<Statement>,
    pub entry: Id<Block>,
    pub return_local: Option<Id<LocalDecl>>,
}

impl Function {
    pub fn alloc_local(&mut self, ty: Id<Type>, span: Span, name: &'static str) -> Id<LocalDecl> {
        self.locals.alloc(LocalDecl { ty, span, name })
    }

    pub fn new_block(&mut self, name: &'static str) -> Id<Block> {
        self.blocks.alloc(Block {
            stmts: Vec::new(),
            terminator: Terminator::Unreachable,
            name,
        })
    }

    pub fn add_stmt(&mut self, stmt: Statement) -> Id<Statement> {
        self.statements.alloc(stmt)
    }
}

use crate::{Expr, Ident, Type, arena::Id};

pub enum Decl {
    Func(Id<Func>),
    Trait(Id<Trait>),
    Impl(Id<Impl>),
    ExternBlock(ExternBlock),
}

pub struct ExternBlock {
    pub library: Option<String>,
    pub functions: Vec<Id<ExternFunc>>,
}

pub struct Func {
    pub name: Ident,
    pub type_parameters: Vec<FuncTypeParam>,
    pub parameters: Vec<FuncParam>,
    pub return_type: Option<Type>,
    pub return_type_id: Option<Id<Type>>, // TypeId for the return type annotation (for span tracking)
    pub body: Id<Expr>,
}

pub struct ExternFunc {
    pub name: Ident,
    pub parameters: Vec<FuncParam>,
    pub return_type: Option<Type>,
}

pub struct Trait {
    pub name: Ident,
    pub parameters: Vec<FuncParam>,
    pub returns: Type,
}

pub struct Impl {
    pub trait_id: Id<Trait>,
    pub self_type: Type,
    pub arg_types: Vec<Type>,
    pub output_type: Type,
}

pub struct FuncParam {
    pub name: Ident,
    pub ty: Option<Type>,
    pub type_id: Option<Id<Type>>,
}

pub struct FuncTypeParam {
    pub name: Ident,
}

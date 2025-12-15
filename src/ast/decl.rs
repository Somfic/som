use crate::{ExprId, Ident, Type, TypeId};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImplId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

pub enum Decl {
    Func(FuncId),
    Trait(TraitId),
    Struct(StructId),
    Impl(ImplId),
}

pub struct Func {
    pub name: Ident,
    pub type_parameters: Vec<FuncTypeParam>,
    pub parameters: Vec<FuncParam>,
    pub return_type: Option<Type>,
    pub return_type_id: Option<TypeId>, // TypeId for the return type annotation (for span tracking)
    pub body: ExprId,
}

pub struct Trait {
    pub name: Ident,
    pub parameters: Vec<FuncParam>,
    pub returns: Type,
}

pub struct Impl {
    pub trait_id: TraitId,
    pub self_type: Type,
    pub arg_types: Vec<Type>,
    pub output_type: Type,
}

pub struct FuncParam {
    pub name: Ident,
    pub ty: Option<Type>,
    pub type_id: Option<TypeId>,
}

pub struct FuncTypeParam {
    pub name: Ident,
}

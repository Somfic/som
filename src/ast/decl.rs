use crate::{Expr, Ident, Type, arena::Id};

pub enum Decl {
    Struct(Id<Struct>),
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

pub struct Struct {
    pub name: Ident,
    pub fields: Vec<StructField>,
}

impl Struct {
    pub fn compute_layout(&self) -> StructLayout {
        fn align_to(offset: usize, alignment: usize) -> usize {
            if alignment == 0 {
                return offset; // Avoid division by zero
            }

            let remainder = offset % alignment;
            if remainder == 0 {
                offset
            } else {
                offset + (alignment - remainder)
            }
        }

        let mut offset = 0;
        let mut field_offsets = Vec::new();
        let mut max_alignment = 1;

        for field in &self.fields {
            let alignment = field.ty.alignment();
            max_alignment = max_alignment.max(alignment);
            offset = align_to(offset, alignment);
            field_offsets.push(offset);
            offset += field.ty.size();
        }

        let size = align_to(offset, max_alignment);

        StructLayout {
            field_offsets,
            size,
            alignment: max_alignment as u8,
        }
    }
}

pub struct StructLayout {
    pub field_offsets: Vec<usize>,
    pub size: usize,
    pub alignment: u8,
}

pub struct StructField {
    pub name: Ident,
    pub ty: Type,
    pub type_id: Id<Type>,
}

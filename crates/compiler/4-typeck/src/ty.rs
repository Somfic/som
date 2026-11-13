use som_ast::Primitive;
use som_base::{Arena, Id};
use som_resolve::{DefId, Names};
use std::collections::HashMap;

/// A type, interned in a `TypeTable`. Equal types have equal ids.
pub type TypeId = Id<Ty>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// The type of something that already failed to check. Unifies with anything,
    /// so one mistake doesn't cascade into more errors.
    Error,
    Unit,
    Bool,
    Char,
    Str,
    Int(IntTy),
    Float(FloatTy),
    /// `*`, an untyped raw pointer.
    Pointer,
    Ref {
        mutable: bool,
        to: TypeId,
    },
    Struct(DefId),
    /// A type parameter, inside the generic function that declares it.
    Param(DefId),
    Fn {
        params: Vec<TypeId>,
        ret: TypeId,
    },
    /// An inference variable. Never appears in the final `Typed` result.
    Var(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatTy {
    F32,
    F64,
}

impl Ty {
    pub fn from_primitive(p: Primitive) -> Ty {
        match p {
            Primitive::I8 => Ty::Int(IntTy::I8),
            Primitive::I16 => Ty::Int(IntTy::I16),
            Primitive::I32 => Ty::Int(IntTy::I32),
            Primitive::I64 => Ty::Int(IntTy::I64),
            Primitive::I128 => Ty::Int(IntTy::I128),
            Primitive::ISize => Ty::Int(IntTy::ISize),
            Primitive::U8 => Ty::Int(IntTy::U8),
            Primitive::U16 => Ty::Int(IntTy::U16),
            Primitive::U32 => Ty::Int(IntTy::U32),
            Primitive::U64 => Ty::Int(IntTy::U64),
            Primitive::U128 => Ty::Int(IntTy::U128),
            Primitive::USize => Ty::Int(IntTy::USize),
            Primitive::F32 => Ty::Float(FloatTy::F32),
            Primitive::F64 => Ty::Float(FloatTy::F64),
            Primitive::Bool => Ty::Bool,
            Primitive::Char => Ty::Char,
            Primitive::Str => Ty::Str,
        }
    }
}

/// Interns types, so they can be compared and copied as plain ids.
#[derive(Clone)]
pub struct TypeTable {
    tys: Arena<Ty>,
    ids: HashMap<Ty, TypeId>,
    pub error: TypeId,
    pub unit: TypeId,
    pub bool: TypeId,
    pub i32: TypeId,
    pub f64: TypeId,
    pub pointer: TypeId,
    pub str_ref: TypeId,
}

impl Default for TypeTable {
    fn default() -> Self {
        let mut table = TypeTable {
            tys: Arena::new(),
            ids: HashMap::new(),
            error: Id::new(0),
            unit: Id::new(0),
            bool: Id::new(0),
            i32: Id::new(0),
            f64: Id::new(0),
            pointer: Id::new(0),
            str_ref: Id::new(0),
        };
        table.error = table.intern(Ty::Error);
        table.unit = table.intern(Ty::Unit);
        table.bool = table.intern(Ty::Bool);
        table.i32 = table.intern(Ty::Int(IntTy::I32));
        table.f64 = table.intern(Ty::Float(FloatTy::F64));
        table.pointer = table.intern(Ty::Pointer);
        let str = table.intern(Ty::Str);
        table.str_ref = table.intern(Ty::Ref {
            mutable: false,
            to: str,
        });
        table
    }
}

impl TypeTable {
    pub fn intern(&mut self, ty: Ty) -> TypeId {
        if let Some(&id) = self.ids.get(&ty) {
            return id;
        }
        let id = self.tys.alloc(ty.clone());
        self.ids.insert(ty, id);
        id
    }

    pub fn get(&self, id: TypeId) -> &Ty {
        &self.tys[id]
    }

    /// The id of `ty` if it has been interned.
    pub fn lookup(&self, ty: &Ty) -> Option<TypeId> {
        self.ids.get(ty).copied()
    }

    /// Renders a type the way it would be written in som.
    pub fn display(&self, id: TypeId, names: &Names) -> String {
        match self.get(id) {
            Ty::Error => "{error}".into(),
            Ty::Unit => "()".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Str => "str".into(),
            Ty::Int(int) => format!("{int:?}").to_lowercase(),
            Ty::Float(float) => format!("{float:?}").to_lowercase(),
            Ty::Pointer => "*".into(),
            Ty::Ref { mutable, to } => {
                format!(
                    "&{}{}",
                    if *mutable { "mut " } else { "" },
                    self.display(*to, names)
                )
            }
            Ty::Struct(def) | Ty::Param(def) => names.def(*def).name.to_string(),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|&p| self.display(p, names)).collect();
                format!("fn({}) -> {}", params.join(", "), self.display(*ret, names))
            }
            Ty::Var(_) => "_".into(),
        }
    }
}

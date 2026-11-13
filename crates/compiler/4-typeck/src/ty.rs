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
    /// Text's bytes. Only ever behind a reference, which holds where they start and how many
    /// there are: som's `str` is `&` of this, like `[u8]` is of a slice.
    Str,
    /// `cstr`: a pointer to text ending in a zero byte, as C takes it.
    CStr,
    Int(IntTy),
    Float(FloatTy),
    /// `*`, an untyped raw pointer.
    Pointer,
    Ref {
        mutable: bool,
        to: TypeId,
    },
    Struct(DefId),
    /// `T?`: a `T`, or `none`.
    Optional(TypeId),
    /// `[T; N]`
    Array {
        element: TypeId,
        len: u64,
    },
    /// Some number of `T`s, known at run time. Only ever behind a reference, which holds where
    /// they start and how many there are: som's `[T]` is `&` of this, and `[mut T]` `&mut`.
    Slice(TypeId),
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
    /// The types written inside this one: what a reference points to, what an optional or
    /// array holds, a function's parameters and result. A struct's fields aren't: a struct is
    /// a name, and its fields belong to its definition.
    ///
    /// With [`Ty::map_children`], this is the one place that knows how types nest, so code
    /// that walks into types doesn't need a case for each kind.
    pub fn children(&self) -> Vec<TypeId> {
        match self {
            Ty::Ref { to: inner, .. }
            | Ty::Optional(inner)
            | Ty::Array { element: inner, .. }
            | Ty::Slice(inner) => vec![*inner],
            Ty::Fn { params, ret } => params.iter().chain([ret]).copied().collect(),
            Ty::Error
            | Ty::Unit
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::CStr
            | Ty::Int(_)
            | Ty::Float(_)
            | Ty::Pointer
            | Ty::Struct(_)
            | Ty::Param(_)
            | Ty::Var(_) => Vec::new(),
        }
    }

    /// This type with each of its [`children`](Ty::children) replaced by `f` of it.
    pub fn map_children(&self, mut f: impl FnMut(TypeId) -> TypeId) -> Ty {
        match self {
            Ty::Ref { mutable, to } => Ty::Ref {
                mutable: *mutable,
                to: f(*to),
            },
            Ty::Optional(inner) => Ty::Optional(f(*inner)),
            Ty::Slice(element) => Ty::Slice(f(*element)),
            Ty::Array { element, len } => Ty::Array {
                element: f(*element),
                len: *len,
            },
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|&p| f(p)).collect(),
                ret: f(*ret),
            },
            leaf => leaf.clone(),
        }
    }

    /// If this type and `other` are the same kind of type, apart from their children (both
    /// `&mut`, both arrays of 3, both functions of two parameters...), the children to match
    /// up, in pairs.
    pub fn zip_children(&self, other: &Ty) -> Option<Vec<(TypeId, TypeId)>> {
        let blank = |ty: &Ty| ty.map_children(|_| Id::new(0));
        (blank(self) == blank(other))
            .then(|| self.children().into_iter().zip(other.children()).collect())
    }

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
            Primitive::CStr => Ty::CStr,
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
    /// `str`: a reference to text's bytes.
    pub str_ref: TypeId,
    pub cstr: TypeId,
    pub u8: TypeId,
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
            cstr: Id::new(0),
            u8: Id::new(0),
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
        table.cstr = table.intern(Ty::CStr);
        table.u8 = table.intern(Ty::Int(IntTy::U8));
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

    /// Every interned type, with its id.
    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &Ty)> {
        self.tys.iter_with_ids()
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
            Ty::CStr => "cstr".into(),
            Ty::Int(int) => format!("{int:?}").to_lowercase(),
            Ty::Float(float) => format!("{float:?}").to_lowercase(),
            Ty::Pointer => "*".into(),
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match self.get(*to) {
                    Ty::Str => "str".into(),
                    Ty::Optional(_) => format!("&{mutability}({})", self.display(*to, names)),
                    Ty::Slice(element) => {
                        format!("[{mutability}{}]", self.display(*element, names))
                    }
                    _ => format!("&{mutability}{}", self.display(*to, names)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match self.get(*inner) {
                Ty::Fn { .. } => format!("({})?", self.display(*inner, names)),
                _ => format!("{}?", self.display(*inner, names)),
            },
            Ty::Array { element, len } => format!("[{}; {len}]", self.display(*element, names)),
            // Only reached on its own for a dereferenced slice, which is an error.
            Ty::Slice(element) => format!("[{}]", self.display(*element, names)),
            Ty::Struct(def) | Ty::Param(def) => names.def(*def).name.to_string(),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|&p| self.display(p, names)).collect();
                format!("fn({}) -> {}", params.join(", "), self.display(*ret, names))
            }
            Ty::Var(_) => "_".into(),
        }
    }
}

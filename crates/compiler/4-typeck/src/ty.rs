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
    /// `*T`: a raw pointer to `T`s. Nothing checks what it points to.
    Ptr(TypeId),
    Ref {
        mutable: bool,
        to: TypeId,
    },
    /// A struct, and its type arguments if it's generic: `Pair<i32>`.
    Struct(DefId, Vec<TypeId>),
    /// `T?`: a `T`, or `none`.
    Optional(TypeId),
    /// `[T; N]`. `len` is a [`Ty::Value`], or a value parameter or variable standing for one.
    Array {
        element: TypeId,
        len: TypeId,
    },
    /// A value standing where a type argument goes: `64` in `Buffer<64>` or `[u8; 64]`. Not
    /// the type of anything: types written with it, like `Buffer<64>` and `Buffer<65>`, differ.
    Value(Value),
    /// `A + B`, of values. Kept in a normal form (see [`TypeTable::normalize`]), so equal sums
    /// are the same type.
    Add(TypeId, TypeId),
    /// `A * B`, of values. Normalized like [`Ty::Add`].
    Mul(TypeId, TypeId),
    /// Some number of `T`s, known at run time. Only ever behind a reference, which holds where
    /// they start and how many there are: som's `[T]` is `&` of this, and `[mut T]` `&mut`.
    Slice(TypeId),
    /// `start..end`: the integers of type `T` from `start` up to, not including, `end`.
    Range(TypeId),
    /// A type parameter, inside the generic function that declares it.
    Param(DefId),
    /// An associated type of `base`'s implementation of trait `trait_`, the `index`th it
    /// declares: `I::Item`. Worked out through the implementation once `base` is known.
    Assoc {
        base: TypeId,
        trait_: DefId,
        index: usize,
    },
    Fn {
        params: Vec<TypeId>,
        ret: TypeId,
    },
    /// An inference variable. Never appears in the final `Typed` result.
    Var(u32),
}

/// What a value parameter can stand for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Int(u128),
    Bool(bool),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
        }
    }
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

impl IntTy {
    /// Whether it can be negative.
    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::I128 | IntTy::ISize
        )
    }
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
            | Ty::Assoc { base: inner, .. }
            | Ty::Ptr(inner)
            | Ty::Optional(inner)
            | Ty::Slice(inner)
            | Ty::Range(inner) => vec![*inner],
            Ty::Array { element, len } => vec![*element, *len],
            Ty::Add(a, b) | Ty::Mul(a, b) => vec![*a, *b],
            Ty::Fn { params, ret } => params.iter().chain([ret]).copied().collect(),
            Ty::Struct(_, args) => args.clone(),
            Ty::Error
            | Ty::Unit
            | Ty::Bool
            | Ty::Char
            | Ty::Str
            | Ty::CStr
            | Ty::Int(_)
            | Ty::Float(_)
            | Ty::Pointer
            | Ty::Param(_)
            | Ty::Value(_)
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
            Ty::Ptr(to) => Ty::Ptr(f(*to)),
            Ty::Assoc {
                base,
                trait_,
                index,
            } => Ty::Assoc {
                base: f(*base),
                trait_: *trait_,
                index: *index,
            },
            Ty::Slice(element) => Ty::Slice(f(*element)),
            Ty::Range(int) => Ty::Range(f(*int)),
            Ty::Array { element, len } => Ty::Array {
                element: f(*element),
                len: f(*len),
            },
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|&p| f(p)).collect(),
                ret: f(*ret),
            },
            Ty::Struct(def, args) => Ty::Struct(*def, args.iter().map(|&a| f(a)).collect()),
            Ty::Add(a, b) => Ty::Add(f(*a), f(*b)),
            Ty::Mul(a, b) => Ty::Mul(f(*a), f(*b)),
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

    /// Whether this is arithmetic on values, `A + B` or `A * B`.
    pub fn is_arithmetic(&self) -> bool {
        matches!(self, Ty::Add(..) | Ty::Mul(..))
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

/// A sum of products of unknowns: each product, its unknowns sorted, and how many times it's
/// added. The empty product is the constant.
pub type Polynomial = std::collections::BTreeMap<Vec<TypeId>, u128>;

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
    pub usize: TypeId,
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
            usize: Id::new(0),
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
        table.usize = table.intern(Ty::Int(IntTy::USize));
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

    /// The number an array's `len` stands for. Only for concrete types, where it's a value.
    pub fn len(&self, len: TypeId) -> u64 {
        match self.get(len) {
            Ty::Value(Value::Int(n)) => *n as u64,
            other => unreachable!("an array's length is a number once it's concrete: {other:?}"),
        }
    }

    /// `id` with any arithmetic in it worked out as far as it can be: numbers added and
    /// multiplied, and what's left in a normal form, a sum of products. `A + B` and `B + A`
    /// normalize to the same type, and so do `2 * N` and `N + N`. Value parameters and
    /// variables are the unknowns, so a variable should be resolved first to be seen through.
    pub fn normalize(&mut self, id: TypeId) -> TypeId {
        match self.get(id).clone() {
            ty if ty.is_arithmetic() => match self.polynomial(id) {
                Some(poly) => self.from_polynomial(poly),
                None => self.error,
            },
            ty if ty.children().is_empty() => id,
            ty => {
                let normalized = ty.map_children(|child| self.normalize(child));
                self.intern(normalized)
            }
        }
    }

    /// `id` as a sum of products: each product of unknowns (sorted, so equal products are
    /// equal) with how many times it's added. `None` if something in it isn't a number.
    pub fn polynomial(&self, id: TypeId) -> Option<Polynomial> {
        let mut poly = Polynomial::new();
        match self.get(id) {
            Ty::Value(Value::Int(n)) => {
                poly.insert(Vec::new(), *n);
            }
            Ty::Add(a, b) => {
                poly = self.polynomial(*a)?;
                for (product, n) in self.polynomial(*b)? {
                    let sum = poly.entry(product).or_default();
                    *sum = sum.checked_add(n)?;
                }
            }
            Ty::Mul(a, b) => {
                let (a, b) = (self.polynomial(*a)?, self.polynomial(*b)?);
                for (x, n) in &a {
                    for (y, m) in &b {
                        let mut product: Vec<TypeId> = x.iter().chain(y).copied().collect();
                        product.sort();
                        let sum = poly.entry(product).or_default();
                        *sum = sum.checked_add(n.checked_mul(*m)?)?;
                    }
                }
            }
            Ty::Param(_) | Ty::Var(_) => {
                poly.insert(vec![id], 1);
            }
            _ => return None,
        }
        poly.retain(|_, n| *n != 0);
        Some(poly)
    }

    /// The type a sum of products is written as: a number if there are no unknowns, and
    /// otherwise each product, its number first, added up in order, the constant last.
    pub fn from_polynomial(&mut self, poly: Polynomial) -> TypeId {
        let mut sum: Option<TypeId> = None;
        // The number goes last, as it's usually written: `N + 1`.
        let (constant, products): (Vec<_>, Vec<_>) = poly
            .into_iter()
            .partition(|(product, _)| product.is_empty());
        for (product, n) in products.into_iter().chain(constant) {
            let mut term =
                (n != 1 || product.is_empty()).then(|| self.intern(Ty::Value(Value::Int(n))));
            for unknown in product {
                term = Some(match term {
                    Some(term) => self.intern(Ty::Mul(term, unknown)),
                    None => unknown,
                });
            }
            let term = term.expect("a product has a number or an unknown");
            sum = Some(match sum {
                Some(sum) => self.intern(Ty::Add(sum, term)),
                None => term,
            });
        }
        sum.unwrap_or_else(|| self.intern(Ty::Value(Value::Int(0))))
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
        self.display_with(id, names, &[])
    }

    /// Renders a type with type parameters shown as what `subst` says they stand for: a
    /// generic struct's field as it is in one use of it, `i32` rather than `T`.
    pub fn display_with(&self, id: TypeId, names: &Names, subst: &[(DefId, TypeId)]) -> String {
        match self.get(id) {
            // Once: what it stands for is in the caller's terms, which can be the same `T`.
            Ty::Param(def) if let Some(&(_, arg)) = subst.iter().find(|(d, _)| d == def) => {
                self.display_with(arg, names, &[])
            }
            Ty::Error => "{unknown}".into(),
            Ty::Unit => "()".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Str => "str".into(),
            Ty::CStr => "cstr".into(),
            Ty::Int(int) => format!("{int:?}").to_lowercase(),
            Ty::Float(float) => format!("{float:?}").to_lowercase(),
            Ty::Pointer => "*".into(),
            Ty::Ptr(to) => format!("*{}", self.display_with(*to, names, subst)),
            Ty::Assoc {
                base,
                trait_,
                index,
            } => format!(
                "{}::{}",
                self.display_with(*base, names, subst),
                assoc_name(names, *trait_, *index)
            ),
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match self.get(*to) {
                    Ty::Str => "str".into(),
                    Ty::Optional(_) => {
                        format!("&{mutability}({})", self.display_with(*to, names, subst))
                    }
                    Ty::Slice(element) => {
                        format!(
                            "[{mutability}{}]",
                            self.display_with(*element, names, subst)
                        )
                    }
                    _ => format!("&{mutability}{}", self.display_with(*to, names, subst)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match self.get(*inner) {
                Ty::Fn { .. } => format!("({})?", self.display_with(*inner, names, subst)),
                _ => format!("{}?", self.display_with(*inner, names, subst)),
            },
            Ty::Array { element, len } => format!(
                "[{}; {}]",
                self.display_with(*element, names, subst),
                self.display_with(*len, names, subst)
            ),
            Ty::Value(value) => value.to_string(),
            Ty::Add(a, b) => format!(
                "{} + {}",
                self.display_with(*a, names, subst),
                self.display_with(*b, names, subst)
            ),
            // A sum inside a product keeps its parentheses.
            Ty::Mul(a, b) => {
                let side = |id: TypeId| match self.get(id) {
                    Ty::Add(..) => format!("({})", self.display_with(id, names, subst)),
                    _ => self.display_with(id, names, subst),
                };
                format!("{} * {}", side(*a), side(*b))
            }
            // Only reached on its own for a dereferenced slice, which is an error.
            Ty::Slice(element) => format!("[{}]", self.display_with(*element, names, subst)),
            Ty::Range(int) => format!("Range<{}>", self.display_with(*int, names, subst)),
            Ty::Struct(def, args) if !args.is_empty() => {
                let args: Vec<String> = args
                    .iter()
                    .map(|&a| self.display_with(a, names, subst))
                    .collect();
                format!("{}<{}>", names.def(*def).name, args.join(", "))
            }
            Ty::Struct(def, _) | Ty::Param(def) => names.def(*def).name.to_string(),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params
                    .iter()
                    .map(|&p| self.display_with(p, names, subst))
                    .collect();
                format!(
                    "fn({}) -> {}",
                    params.join(", "),
                    self.display_with(*ret, names, subst)
                )
            }
            Ty::Var(_) => "_".into(),
        }
    }
}

/// The name of trait `trait_`'s `index`th associated type: `Item`.
pub fn assoc_name(names: &Names, trait_: DefId, index: usize) -> String {
    match names.def(trait_).kind {
        som_resolve::DefKind::Trait(t) => names
            .trait_types(t)
            .get(index)
            .map(|&d| names.def(d).name.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

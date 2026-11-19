use som_ast::Primitive;
use som_base::{Arena, Id};
use som_resolve::{DefId, Names};
use std::collections::HashMap;

use crate::unify::Unifier;

/// Interned: equal types have equal ids.
pub type TypeId = Id<Ty>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// Unifies with anything, so one mistake doesn't cascade into more errors.
    Error,
    Unit,
    Bool,
    Char,
    /// Text's bytes. Only ever behind a reference, which holds where they start and how many
    /// there are: som's `str` is `&` of this, like `[u8]` is of a slice.
    Str,
    /// A pointer to text ending in a zero byte, as C takes it.
    CStr,
    Int(IntTy),
    Float(FloatTy),
    /// An untyped raw pointer.
    Pointer,
    /// Nothing checks what it points to.
    Ptr(TypeId),
    Ref {
        mutable: bool,
        to: TypeId,
    },
    Struct(DefId, Vec<TypeId>),
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
    /// A function's own type: `is_even`'s, no other's. It holds nothing, since the type says
    /// which function it is, so a call through it is a call of that function. It becomes a
    /// `Fn`, the function's address, where one is expected. With a generic function's type
    /// arguments.
    FnItem(DefId, Vec<TypeId>),
    /// An inference variable. Never appears in the final `Typed` result.
    Var(u32),
}

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

pub use som_ast::{FloatTy, IntTy};

impl Ty {
    /// A struct's fields aren't children: a struct is a name, and its fields belong to its
    /// definition. With `map_children`, the one place that knows how types nest.
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
            Ty::Struct(_, args) | Ty::FnItem(_, args) => args.clone(),
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
            Ty::FnItem(def, args) => Ty::FnItem(*def, args.iter().map(|&a| f(a)).collect()),
            Ty::Add(a, b) => Ty::Add(f(*a), f(*b)),
            Ty::Mul(a, b) => Ty::Mul(f(*a), f(*b)),
            leaf => leaf.clone(),
        }
    }

    /// If both are the same kind apart from their children (both `&mut`, both arrays of 3...),
    /// the children paired up.
    pub fn zip_children(&self, other: &Ty) -> Option<Vec<(TypeId, TypeId)>> {
        let blank = |ty: &Ty| ty.map_children(|_| Id::new(0));
        (blank(self) == blank(other))
            .then(|| self.children().into_iter().zip(other.children()).collect())
    }

    pub fn is_arithmetic(&self) -> bool {
        matches!(self, Ty::Add(..) | Ty::Mul(..))
    }

    pub fn from_primitive(p: Primitive) -> Ty {
        if let Some(int) = p.int() {
            return Ty::Int(int);
        }
        if let Some(float) = p.float() {
            return Ty::Float(float);
        }
        match p {
            Primitive::Bool => Ty::Bool,
            Primitive::Char => Ty::Char,
            Primitive::Str => Ty::Str,
            Primitive::CStr => Ty::CStr,
            _ => unreachable!("numbers are handled above"),
        }
    }
}

/// A sum of products of unknowns: each product, its unknowns sorted, and how many times it's
/// added. The empty product is the constant.
pub type Polynomial = std::collections::BTreeMap<Vec<TypeId>, u128>;

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
    /// What text is: the standard library's `string`, once it's loaded, or else `&str`.
    pub text: TypeId,
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
            text: Id::new(0),
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
        table.text = table.str_ref;
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

    /// Only for concrete types. An error length (`0` stands in) is one still to be worked out:
    /// code lowered with it, to run a constant, is thrown away.
    pub fn len(&self, len: TypeId) -> u64 {
        match self.get(len) {
            Ty::Value(Value::Int(n)) => *n as u64,
            Ty::Error => 0,
            other => unreachable!("an array's length is a number once it's concrete: {other:?}"),
        }
    }

    /// Arithmetic worked out as far as it can be, the rest in a normal form (a sum of
    /// products), so `A + B` and `B + A` are one type, as are `2 * N` and `N + N`. Variables
    /// are unknowns, so resolve one first to see through it.
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

    /// `None` if something in it isn't a number.
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

    pub fn iter(&self) -> impl Iterator<Item = (TypeId, &Ty)> {
        self.tys.iter_with_ids()
    }

    pub fn lookup(&self, ty: &Ty) -> Option<TypeId> {
        self.ids.get(ty).copied()
    }

    pub fn display(&self, id: TypeId, names: &Names) -> String {
        self.display_with(id, names, &[])
    }

    /// Type parameters shown as what `subst` says they stand for: `i32` rather than `T`.
    pub fn display_with(&self, id: TypeId, names: &Names, subst: &[(DefId, TypeId)]) -> String {
        self.render(id, names, subst, None)
    }

    /// With `vars`, a type still being inferred: variables are followed to what they're bound
    /// to, and one that isn't yet is shown as what it would become.
    pub(crate) fn render(
        &self,
        id: TypeId,
        names: &Names,
        subst: &[(DefId, TypeId)],
        vars: Option<&Unifier>,
    ) -> String {
        let get = |id: TypeId| self.get(vars.map_or(id, |u| u.shallow(id)));
        let show = |id: TypeId| self.render(id, names, subst, vars);
        match get(id) {
            // Once: what it stands for is in the caller's terms, which can be the same `T`.
            Ty::Param(def) if let Some(arg) = crate::impls::substituted(subst, *def) => {
                self.render(arg, names, &[], vars)
            }
            Ty::Error => "{unknown}".into(),
            Ty::Unit => "()".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Str => "str".into(),
            Ty::CStr => "cstr".into(),
            Ty::Int(int) => int.name().to_string(),
            Ty::Float(float) => float.name().to_string(),
            Ty::Pointer => "*".into(),
            Ty::Ptr(to) => format!("*{}", show(*to)),
            Ty::Assoc {
                base,
                trait_,
                index,
            } => format!("{}::{}", show(*base), assoc_name(names, *trait_, *index)),
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match get(*to) {
                    Ty::Str => "str".into(),
                    Ty::Optional(_) => format!("&{mutability}({})", show(*to)),
                    Ty::Slice(element) => format!("&{mutability}[{}]", show(*element)),
                    _ => format!("&{mutability}{}", show(*to)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match get(*inner) {
                Ty::Fn { .. } => format!("({})?", show(*inner)),
                _ => format!("{}?", show(*inner)),
            },
            Ty::Array { element, len } => format!("[{}; {}]", show(*element), show(*len)),
            Ty::Value(value) => value.to_string(),
            Ty::Add(a, b) => format!("{} + {}", show(*a), show(*b)),
            Ty::Mul(a, b) => {
                let side = |id: TypeId| match get(id) {
                    Ty::Add(..) => format!("({})", show(id)),
                    _ => show(id),
                };
                format!("{} * {}", side(*a), side(*b))
            }
            // Only reached on its own for a dereferenced slice, which is an error.
            Ty::Slice(element) => format!("[{}]", show(*element)),
            Ty::Range(int) => format!("Range<{}>", show(*int)),
            Ty::Struct(def, args) if !args.is_empty() => {
                let args: Vec<String> = args.iter().map(|&a| show(a)).collect();
                format!("{}<{}>", names.def(*def).name, args.join(", "))
            }
            Ty::Struct(def, _) | Ty::Param(def) => names.def(*def).name.to_string(),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|&p| show(p)).collect();
                format!("fn({}) -> {}", params.join(", "), show(*ret))
            }
            // Named as in Rust's messages: `fn is_even`.
            Ty::FnItem(def, _) => format!("fn {}", names.def(*def).name),
            Ty::Var(v) => match vars {
                Some(u) => u.display_var(*v, names),
                None => "_".into(),
            },
        }
    }
}

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

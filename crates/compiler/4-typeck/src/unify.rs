//! Inference variables and unification.

use crate::ty::{Ty, TypeId, TypeTable};
use som_resolve::Names;

/// What an unbound inference variable may still become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarKind {
    Any,
    /// From an integer literal: any integer type. Defaults to `i32`.
    Int,
    /// From a float literal: any float type. Defaults to `f64`.
    Float,
    /// From a string literal: a `str`, or a `cstr` for C. Defaults to `str`.
    Text,
    /// From a collection literal, `[a, b, c]` or `[v; N]`: `len` elements of type `element`,
    /// in an array of that length or a slice. Defaults to the array.
    Collection {
        element: TypeId,
        len: u64,
    },
}

struct Var {
    bound: Option<TypeId>,
    kind: VarKind,
}

pub struct Unifier {
    pub table: TypeTable,
    vars: Vec<Var>,
}

impl Unifier {
    pub fn new() -> Self {
        Unifier {
            table: TypeTable::default(),
            vars: Vec::new(),
        }
    }

    pub fn fresh(&mut self, kind: VarKind) -> TypeId {
        let var = self.vars.len() as u32;
        self.vars.push(Var { bound: None, kind });
        self.table.intern(Ty::Var(var))
    }

    pub fn intern(&mut self, ty: Ty) -> TypeId {
        self.table.intern(ty)
    }

    pub fn get(&self, id: TypeId) -> &Ty {
        self.table.get(id)
    }

    /// Follows bound variables until reaching a type that isn't a bound variable.
    pub fn shallow(&self, mut id: TypeId) -> TypeId {
        while let Ty::Var(v) = self.table.get(id)
            && let Some(bound) = self.vars[*v as usize].bound
        {
            id = bound;
        }
        id
    }

    /// The kind of `id` if it's an unbound variable.
    pub fn var_kind(&self, id: TypeId) -> Option<VarKind> {
        match self.table.get(self.shallow(id)) {
            Ty::Var(v) => Some(self.vars[*v as usize].kind),
            _ => None,
        }
    }

    /// Makes `a` and `b` the same type, or fails without changing anything visible.
    pub fn unify(&mut self, a: TypeId, b: TypeId) -> Result<(), ()> {
        let (a, b) = (self.shallow(a), self.shallow(b));
        if a == b {
            return Ok(());
        }
        match (self.table.get(a).clone(), self.table.get(b).clone()) {
            (Ty::Error, _) | (_, Ty::Error) => Ok(()),
            (Ty::Var(v), Ty::Var(w)) => {
                // Two collections: the same number of the same elements.
                if let (
                    VarKind::Collection { element: x, len: n },
                    VarKind::Collection { element: y, len: m },
                ) = (self.vars[v as usize].kind, self.vars[w as usize].kind)
                {
                    if n != m {
                        return Err(());
                    }
                    self.unify(x, y)?;
                    self.vars[v as usize].bound = Some(b);
                    return Ok(());
                }
                let kind =
                    merge(self.vars[v as usize].kind, self.vars[w as usize].kind).ok_or(())?;
                self.vars[w as usize].kind = kind;
                self.vars[v as usize].bound = Some(b);
                Ok(())
            }
            (Ty::Var(v), _) => self.bind(v, b),
            (_, Ty::Var(w)) => self.bind(w, a),
            // The same kind of type: their children have to match too.
            (a, b) => match a.zip_children(&b) {
                Some(pairs) => pairs.into_iter().try_for_each(|(x, y)| self.unify(x, y)),
                None => Err(()),
            },
        }
    }

    fn bind(&mut self, var: u32, ty: TypeId) -> Result<(), ()> {
        let fits = match self.vars[var as usize].kind {
            VarKind::Any => true,
            VarKind::Int => matches!(self.table.get(ty), Ty::Int(_)),
            VarKind::Float => matches!(self.table.get(ty), Ty::Float(_)),
            VarKind::Text => ty == self.table.str_ref || ty == self.table.cstr,
            // An array of as many, or a slice, of the same elements.
            VarKind::Collection { element, len } => {
                let found = match self.table.get(ty).clone() {
                    Ty::Array { element, len: n } if n == len => Some(element),
                    Ty::Ref { to, .. } => match self.table.get(self.shallow(to)) {
                        Ty::Slice(element) => Some(*element),
                        _ => None,
                    },
                    _ => None,
                };
                match found {
                    Some(found) => self.unify(element, found).is_ok(),
                    None => false,
                }
            }
        };
        if !fits || self.occurs(var, ty) {
            return Err(());
        }
        self.vars[var as usize].bound = Some(ty);
        Ok(())
    }

    fn occurs(&self, var: u32, ty: TypeId) -> bool {
        match self.table.get(self.shallow(ty)) {
            Ty::Var(v) if *v == var => true,
            Ty::Var(v) => match self.vars[*v as usize].kind {
                VarKind::Collection { element, .. } => self.occurs(var, element),
                _ => false,
            },
            ty => ty
                .children()
                .into_iter()
                .any(|child| self.occurs(var, child)),
        }
    }

    /// Replaces every bound variable in `id`. Unbound integer and float variables
    /// get their defaults; any other unbound variable is returned in `unresolved`.
    pub fn finish(&mut self, id: TypeId, unresolved: &mut Option<TypeId>) -> TypeId {
        let id = self.shallow(id);
        match self.table.get(id).clone() {
            Ty::Var(v) => match self.vars[v as usize].kind {
                VarKind::Int => {
                    self.vars[v as usize].bound = Some(self.table.i32);
                    self.table.i32
                }
                VarKind::Float => {
                    self.vars[v as usize].bound = Some(self.table.f64);
                    self.table.f64
                }
                VarKind::Any => {
                    unresolved.get_or_insert(id);
                    self.table.error
                }
                VarKind::Text => {
                    self.vars[v as usize].bound = Some(self.table.str_ref);
                    self.table.str_ref
                }
                VarKind::Collection { element, len } => {
                    let element = self.finish(element, unresolved);
                    let array = self.intern(Ty::Array { element, len });
                    self.vars[v as usize].bound = Some(array);
                    array
                }
            },
            ty => {
                if ty.children().is_empty() {
                    return id;
                }
                let finished = ty.map_children(|child| self.finish(child, unresolved));
                self.intern(finished)
            }
        }
    }

    /// Renders a type that may still contain variables, for error messages.
    pub fn display(&self, id: TypeId, names: &Names) -> String {
        let id = self.shallow(id);
        match self.table.get(id) {
            // Only reached for a variable inside another type, like `&_`: on its own, the
            // checker describes it in words instead. A literal's type is shown as what it
            // becomes if nothing else decides it: `maybe(1)` is an `i32?`.
            Ty::Var(v) => match self.vars[*v as usize].kind {
                VarKind::Int => "i32".into(),
                VarKind::Float => "f64".into(),
                VarKind::Any => "_".into(),
                VarKind::Text => "str".into(),
                VarKind::Collection { element, len } => {
                    format!("[{}; {len}]", self.display(element, names))
                }
            },
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match self.table.get(self.shallow(*to)) {
                    Ty::Str => "str".into(),
                    Ty::Optional(_) => format!("&{mutability}({})", self.display(*to, names)),
                    Ty::Slice(element) => {
                        format!("[{mutability}{}]", self.display(*element, names))
                    }
                    _ => format!("&{mutability}{}", self.display(*to, names)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match self.table.get(self.shallow(*inner)) {
                Ty::Fn { .. } => format!("({})?", self.display(*inner, names)),
                _ => format!("{}?", self.display(*inner, names)),
            },
            Ty::Array { element, len } => format!("[{}; {len}]", self.display(*element, names)),
            Ty::Slice(element) => format!("[{}]", self.display(*element, names)),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|&p| self.display(p, names)).collect();
                format!("fn({}) -> {}", params.join(", "), self.display(*ret, names))
            }
            _ => self.table.display(id, names),
        }
    }
}

fn merge(a: VarKind, b: VarKind) -> Option<VarKind> {
    match (a, b) {
        (VarKind::Any, k) | (k, VarKind::Any) => Some(k),
        (a, b) if a == b => Some(a),
        _ => None,
    }
}

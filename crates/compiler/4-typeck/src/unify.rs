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
                let kind =
                    merge(self.vars[v as usize].kind, self.vars[w as usize].kind).ok_or(())?;
                self.vars[w as usize].kind = kind;
                self.vars[v as usize].bound = Some(b);
                Ok(())
            }
            (Ty::Var(v), _) => self.bind(v, b),
            (_, Ty::Var(w)) => self.bind(w, a),
            (
                Ty::Ref {
                    mutable: m1,
                    to: t1,
                },
                Ty::Ref {
                    mutable: m2,
                    to: t2,
                },
            ) if m1 == m2 => self.unify(t1, t2),
            (Ty::Optional(a), Ty::Optional(b)) => self.unify(a, b),
            (
                Ty::Fn {
                    params: p1,
                    ret: r1,
                },
                Ty::Fn {
                    params: p2,
                    ret: r2,
                },
            ) if p1.len() == p2.len() => {
                for (x, y) in p1.into_iter().zip(p2) {
                    self.unify(x, y)?;
                }
                self.unify(r1, r2)
            }
            _ => Err(()),
        }
    }

    fn bind(&mut self, var: u32, ty: TypeId) -> Result<(), ()> {
        let fits = match self.vars[var as usize].kind {
            VarKind::Any => true,
            VarKind::Int => matches!(self.table.get(ty), Ty::Int(_)),
            VarKind::Float => matches!(self.table.get(ty), Ty::Float(_)),
        };
        if !fits || self.occurs(var, ty) {
            return Err(());
        }
        self.vars[var as usize].bound = Some(ty);
        Ok(())
    }

    fn occurs(&self, var: u32, ty: TypeId) -> bool {
        match self.table.get(self.shallow(ty)) {
            Ty::Var(v) => *v == var,
            Ty::Ref { to, .. } | Ty::Optional(to) => self.occurs(var, *to),
            Ty::Fn { params, ret } => {
                params.iter().any(|&p| self.occurs(var, p)) || self.occurs(var, *ret)
            }
            _ => false,
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
            },
            Ty::Ref { mutable, to } => {
                let to = self.finish(to, unresolved);
                self.intern(Ty::Ref { mutable, to })
            }
            Ty::Optional(inner) => {
                let inner = self.finish(inner, unresolved);
                self.intern(Ty::Optional(inner))
            }
            Ty::Fn { params, ret } => {
                let params = params
                    .into_iter()
                    .map(|p| self.finish(p, unresolved))
                    .collect();
                let ret = self.finish(ret, unresolved);
                self.intern(Ty::Fn { params, ret })
            }
            _ => id,
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
            },
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match self.table.get(self.shallow(*to)) {
                    Ty::Optional(_) => format!("&{mutability}({})", self.display(*to, names)),
                    _ => format!("&{mutability}{}", self.display(*to, names)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match self.table.get(self.shallow(*inner)) {
                Ty::Fn { .. } => format!("({})?", self.display(*inner, names)),
                _ => format!("{}?", self.display(*inner, names)),
            },
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

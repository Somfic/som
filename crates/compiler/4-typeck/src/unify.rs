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
    /// in an array of that length or a slice. Defaults to the array. `len` is an array's.
    Collection {
        element: TypeId,
        len: TypeId,
    },
}

struct Var {
    bound: Option<TypeId>,
    kind: VarKind,
}

pub struct Unifier {
    pub table: TypeTable,
    vars: Vec<Var>,
    /// Arithmetic that can't be decided yet, like `?R * ?C` against `R * C`, to check again
    /// once more of its variables are known. See [`Unifier::settle`].
    deferred: Vec<(TypeId, TypeId)>,
}

/// What comparing two values that involve arithmetic found.
enum Arithmetic {
    /// They're equal, or now are: a variable was bound to make them.
    Equal,
    /// They can't be equal.
    Unequal,
    /// It depends on variables that aren't known yet.
    Undecided,
}

impl Unifier {
    pub fn new() -> Self {
        Unifier {
            table: TypeTable::default(),
            vars: Vec::new(),
            deferred: Vec::new(),
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
                    self.unify(n, m)?;
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
            (x, y) if x.is_arithmetic() || y.is_arithmetic() => self.unify_arithmetic(a, b),
            // The same kind of type: their children have to match too.
            (a, b) => match a.zip_children(&b) {
                Some(pairs) => pairs.into_iter().try_for_each(|(x, y)| self.unify(x, y)),
                None => Err(()),
            },
        }
    }

    /// Whether `found` could be used where `expected` is, without changing anything: as it is,
    /// a `T` as a `T?`, or a reference to an array as a slice. An unknown counts as fitting
    /// whatever its kind allows. For choosing between functions with the same name.
    pub fn could_fit(&self, expected: TypeId, found: TypeId) -> bool {
        let (e, f) = (self.shallow(expected), self.shallow(found));
        if e == f {
            return true;
        }
        let (wanted, given) = (self.table.get(e), self.table.get(f));
        match (wanted, given) {
            (Ty::Error, _) | (_, Ty::Error) => true,
            (Ty::Var(v), _) => self.kind_allows(self.vars[*v as usize].kind, f),
            (_, Ty::Var(w)) => self.kind_allows(self.vars[*w as usize].kind, e),
            (x, y) if x.is_arithmetic() || y.is_arithmetic() => true,
            (Ty::Optional(inner), given) if !matches!(given, Ty::Optional(_)) => {
                self.could_fit(*inner, f)
            }
            (Ty::Ref { to: slice, .. }, Ty::Ref { to: array, .. })
                if matches!(self.table.get(self.shallow(*slice)), Ty::Slice(_))
                    && matches!(self.table.get(self.shallow(*array)), Ty::Array { .. }) =>
            {
                true
            }
            (x, y) => match x.zip_children(y) {
                Some(pairs) => pairs.into_iter().all(|(a, b)| self.could_fit(a, b)),
                None => false,
            },
        }
    }

    /// Whether an unknown of `kind` could turn out to be `ty`.
    fn kind_allows(&self, kind: VarKind, ty: TypeId) -> bool {
        let ty = self.shallow(ty);
        match (kind, self.table.get(ty)) {
            (_, Ty::Var(_) | Ty::Error) | (VarKind::Any, _) => true,
            // Wrapped in the optional where it's used.
            (kind, Ty::Optional(inner)) => self.kind_allows(kind, *inner),
            (VarKind::Int, t) => matches!(t, Ty::Int(_)),
            (VarKind::Float, t) => matches!(t, Ty::Float(_)),
            (VarKind::Text, t) => matches!(t, Ty::CStr) || ty == self.table.str_ref,
            (VarKind::Collection { .. }, Ty::Array { .. }) => true,
            (VarKind::Collection { .. }, Ty::Ref { to, .. }) => {
                matches!(self.table.get(self.shallow(*to)), Ty::Slice(_))
            }
            (VarKind::Collection { .. }, _) => false,
        }
    }

    /// `id` with every bound variable in it replaced by what it's bound to.
    fn resolved(&mut self, id: TypeId) -> TypeId {
        let id = self.shallow(id);
        let ty = self.table.get(id).clone();
        if ty.children().is_empty() {
            return id;
        }
        let resolved = ty.map_children(|child| self.resolved(child));
        self.table.intern(resolved)
    }

    /// `id` with its arithmetic worked out as far as what's known so far allows.
    pub fn normalize(&mut self, id: TypeId) -> TypeId {
        let resolved = self.resolved(id);
        self.table.normalize(resolved)
    }

    /// Makes two values equal where at least one is arithmetic: `A + B` and `B + A`, or
    /// `N + 1` and `4`, which makes `N` 3. What depends on variables not known yet is checked
    /// again later, by [`Unifier::settle`].
    fn unify_arithmetic(&mut self, a: TypeId, b: TypeId) -> Result<(), ()> {
        match self.compare_arithmetic(a, b) {
            Arithmetic::Equal => Ok(()),
            Arithmetic::Unequal => Err(()),
            Arithmetic::Undecided => {
                self.deferred.push((a, b));
                Ok(())
            }
        }
    }

    /// Checks the arithmetic [`Unifier::unify`] couldn't decide before, now that more may be
    /// known, until nothing changes. Returns the pairs that turned out unequal; what still
    /// can't be decided is kept for next time.
    pub fn settle(&mut self) -> Vec<(TypeId, TypeId)> {
        let mut unequal = Vec::new();
        loop {
            let mut progress = false;
            for (a, b) in std::mem::take(&mut self.deferred) {
                match self.compare_arithmetic(a, b) {
                    Arithmetic::Equal => progress = true,
                    Arithmetic::Unequal => unequal.push((a, b)),
                    Arithmetic::Undecided => self.deferred.push((a, b)),
                }
            }
            if !progress {
                return unequal;
            }
        }
    }

    /// Whether `a` and `b` are equal. A variable that's added, not multiplied by another, is
    /// worked out if it's the only one left: `N + 1 = 4` makes `N` 3, but `N * M = 6` has many
    /// answers.
    fn compare_arithmetic(&mut self, a: TypeId, b: TypeId) -> Arithmetic {
        let (a, b) = (self.normalize(a), self.normalize(b));
        if a == b {
            return Arithmetic::Equal;
        }
        // Worked out to something that isn't arithmetic any more, like a number.
        if !self.table.get(a).is_arithmetic() && !self.table.get(b).is_arithmetic() {
            return match self.unify(a, b) {
                Ok(()) => Arithmetic::Equal,
                Err(()) => Arithmetic::Unequal,
            };
        }
        let (Some(x), Some(y)) = (self.table.polynomial(a), self.table.polynomial(b)) else {
            return Arithmetic::Unequal;
        };
        // `x - y = 0`. Solvable here if it comes down to `factor * v + constant = 0`.
        let mut difference: std::collections::BTreeMap<Vec<TypeId>, i128> = x
            .into_iter()
            .map(|(product, n)| (product, n as i128))
            .collect();
        for (product, n) in y {
            *difference.entry(product).or_default() -= n as i128;
        }
        difference.retain(|_, n| *n != 0);
        let has_vars = difference
            .keys()
            .flatten()
            .any(|&unknown| matches!(self.table.get(unknown), Ty::Var(_)));
        let constant = difference.remove(&Vec::new()).unwrap_or(0);
        if let [(product, factor)] = &difference.iter().collect::<Vec<_>>()[..]
            && let [unknown] = product[..]
            && let Ty::Var(var) = *self.table.get(unknown)
        {
            let factor = **factor;
            if (-constant) % factor != 0 || (-constant) / factor < 0 {
                return Arithmetic::Unequal;
            }
            let value = (-constant / factor) as u128;
            let value = self.table.intern(Ty::Value(crate::ty::Value::Int(value)));
            return match self.bind(var, value) {
                Ok(()) => Arithmetic::Equal,
                Err(()) => Arithmetic::Unequal,
            };
        }
        match has_vars {
            true => Arithmetic::Undecided,
            false => Arithmetic::Unequal,
        }
    }

    /// Makes `ty`, if it's still unknown, the error type: what it is was already reported as
    /// wrong, so nothing more is said about it.
    /// Anything still unknown inside it, too: `?T` of an `?T?` it was compared with `none` as.
    pub fn poison(&mut self, ty: TypeId) {
        let ty = self.shallow(ty);
        match self.table.get(ty).clone() {
            Ty::Var(v) => self.vars[v as usize].bound = Some(self.table.error),
            other => other
                .children()
                .into_iter()
                .for_each(|child| self.poison(child)),
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
                    Ty::Array { element, len: n } if self.unify(len, n).is_ok() => Some(element),
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
                VarKind::Collection { element, len } => {
                    self.occurs(var, element) || self.occurs(var, len)
                }
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
                    let len = self.finish(len, unresolved);
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
                let finished = self.intern(finished);
                self.table.normalize(finished)
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
                    format!(
                        "[{}; {}]",
                        self.display(element, names),
                        self.display(len, names)
                    )
                }
            },
            Ty::Ptr(to) => format!("*{}", self.display(*to, names)),
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
            Ty::Array { element, len } => format!(
                "[{}; {}]",
                self.display(*element, names),
                self.display(*len, names)
            ),
            Ty::Slice(element) => format!("[{}]", self.display(*element, names)),
            Ty::Struct(def, args) if !args.is_empty() => {
                let args: Vec<String> = args.iter().map(|&a| self.display(a, names)).collect();
                format!("{}<{}>", names.def(*def).name, args.join(", "))
            }
            Ty::Range(int) => format!("Range<{}>", self.display(*int, names)),
            Ty::Add(a, b) => format!("{} + {}", self.display(*a, names), self.display(*b, names)),
            Ty::Mul(a, b) => {
                let side = |id: TypeId| match self.table.get(self.shallow(id)) {
                    Ty::Add(..) => format!("({})", self.display(id, names)),
                    _ => self.display(id, names),
                };
                format!("{} * {}", side(*a), side(*b))
            }
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

//! Anonymous functions: `n => *n > limit`. Each has a type of its own, holding what it
//! captures, and is checked where it's written, reading the variables around it.

use som_ast::{Expr, Id, Param};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::ClosureSig;
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

impl Checker<'_> {
    /// `(a, b) => body`: its parameters' types are written, or what calling it is expected to
    /// take; its body is checked with them, giving what it returns.
    pub(super) fn infer_closure(&mut self, id: Id<Expr>, params: &[Param], body: Id<Expr>) -> TypeId {
        let expected = self.hints.expected_here.take();
        let wanted = expected.and_then(|ty| self.wanted_signature(ty, params.len()));
        let (wanted_params, wanted_ret) = match wanted {
            Some((params, ret)) => (Some(params), Some(ret)),
            None => (None, None),
        };
        let defs = self.names.closure_params(id).to_vec();
        let mut types = Vec::new();
        for (index, (param, &def)) in params.iter().zip(&defs).enumerate() {
            let wanted = wanted_params.as_ref().map(|w| w[index]);
            let ty = match param.ty {
                Some(written) => {
                    let ty = self.lower(written);
                    if let Some(wanted) = wanted {
                        let _ = self.u.unify(wanted, ty);
                    }
                    ty
                }
                None => wanted.unwrap_or_else(|| self.u.fresh(VarKind::Any)),
            };
            self.locals.insert(def, ty);
            types.push(ty);
        }
        let ret = wanted_ret.unwrap_or_else(|| self.u.fresh(VarKind::Any));

        // Its own body: a `return` leaves it, and a `break` can't reach a loop around it.
        let returns = self.returns.replace((ret, None));
        let loop_depth = std::mem::take(&mut self.loop_depth);
        self.open_closures.push(id);
        self.expect(body, ret, None);
        self.open_closures.pop();
        self.loop_depth = loop_depth;
        self.returns = returns;

        let captures = (self.names.captures(id).iter())
            .map(|&def| (def, self.locals.get(&def).copied().unwrap_or(self.u.table.error)))
            .collect();
        let owner = self.current_func;
        self.records.closures.insert(
            id,
            ClosureSig {
                owner,
                params: types,
                ret,
                captures,
            },
        );
        let def = self.names.closure(id).expect("closures are resolved");
        let args = match owner {
            Some(func) => (self.names.type_params(func).iter())
                .map(|&p| match self.bound.get(&p) {
                    Some(&bound) => bound,
                    None => self.u.intern(Ty::Param(p)),
                })
                .collect(),
            None => Vec::new(),
        };
        self.u.intern(Ty::Closure(def, args))
    }

    /// What a closure written where `expected` is expected should take and give, if that's
    /// known and takes `count` values: a `fn(…) -> …`, or the type parameter of a call it's
    /// given for, `F: fn(&T) -> bool`.
    fn wanted_signature(&mut self, expected: TypeId, count: usize) -> Option<(Vec<TypeId>, TypeId)> {
        let expected = self.u.shallow(expected);
        let signature = match self.u.get(expected) {
            Ty::Fn { .. } => expected,
            Ty::Var(_) => *self.expected_calls.get(&expected)?,
            _ => self.signature_of(expected)?,
        };
        let signature = self.normalize_assoc(signature);
        match self.u.shallow_ty(signature).clone() {
            Ty::Fn { params, ret } if params.len() == count => {
                let params = params.into_iter().map(|p| self.normalize_assoc(p)).collect();
                Some((params, ret))
            }
            _ => None,
        }
    }

    /// What closure `def` takes and gives, as a `fn(…) -> …`, with `args` for the type
    /// parameters of the function it's in.
    pub(super) fn closure_signature(&mut self, def: DefId, args: &[TypeId]) -> Option<TypeId> {
        let DefKind::Closure(expr) = self.names.def(def).kind else {
            return None;
        };
        let sig = self.records.closures.get(&expr)?.clone();
        let subst: Vec<(DefId, TypeId)> = match sig.owner {
            Some(func) => (self.names.type_params(func).iter().copied())
                .zip(args.iter().copied())
                .collect(),
            None => Vec::new(),
        };
        let params = (sig.params.iter())
            .map(|&p| self.substitute(p, &subst))
            .collect();
        let ret = self.substitute(sig.ret, &subst);
        Some(self.u.intern(Ty::Fn { params, ret }))
    }
}

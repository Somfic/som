use std::fmt::Debug;
use std::ops::Index;

use ena::unify::InPlaceUnificationTable;
use som_common::{Arena, Id, Span};

use crate::{Type, TypeValue, TypeVar};

pub struct TyCtx {
    types: Arena<Type>,
    table: InPlaceUnificationTable<TypeVar>,
}

impl Debug for TyCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TyCtx").field("types", &self.types).finish()
    }
}

impl Index<Id<Type>> for TyCtx {
    type Output = Type;
    fn index(&self, id: Id<Type>) -> &Type {
        self.types.get(&id)
    }
}

impl TyCtx {
    pub(crate) fn new() -> Self {
        Self {
            types: Arena::new(),
            table: InPlaceUnificationTable::new(),
        }
    }

    pub(crate) fn error(&mut self, span: Span) -> Id<Type> {
        self.types.alloc(Type::Error { span })
    }

    pub(crate) fn bool(&mut self, span: Span) -> Id<Type> {
        self.types.alloc(Type::Bool { span })
    }

    pub(crate) fn nothing(&mut self, span: Span) -> Id<Type> {
        self.types.alloc(Type::Nothing { span })
    }

    pub(crate) fn int(&mut self, span: Span) -> Id<Type> {
        let var = self.table.new_key(TypeValue::Unbound { is_int: true });
        self.types.alloc(Type::Infer { var, span })
    }

    /// A fresh, fully unconstrained inference variable (any type).
    pub(crate) fn var(&mut self, span: Span) -> Id<Type> {
        let var = self.table.new_key(TypeValue::Unbound { is_int: false });
        self.types.alloc(Type::Infer { var, span })
    }

    pub(crate) fn unify(&mut self, a: Id<Type>, b: Id<Type>) -> Result<(), ()> {
        let (ta, tb) = (self.types[a], self.types[b]);
        match (ta, tb) {
            (Type::Error { .. }, _) | (_, Type::Error { .. }) => Ok(()),
            (Type::Infer { var: va, .. }, Type::Infer { var: vb, .. }) => {
                self.table.unify_var_var(va, vb).map_err(|_| ())
            }
            (Type::Infer { var, .. }, concrete) | (concrete, Type::Infer { var, .. }) => self
                .table
                .unify_var_value(var, prim(concrete))
                .map_err(|_| ()),
            (a_ty, b_ty) => {
                if prim(a_ty) == prim(b_ty) {
                    Ok(())
                } else {
                    Err(())
                }
            }
        }
    }

    pub(crate) fn describe(&mut self, id: Id<Type>) -> &'static str {
        match self.types[id] {
            Type::I32 { .. } => "i32",
            Type::Bool { .. } => "bool",
            Type::Nothing { .. } => "nothing",
            Type::Error { .. } => "<error>",
            Type::Infer { var, .. } => match self.table.probe_value(var) {
                TypeValue::I32 => "i32",
                TypeValue::Bool => "bool",
                TypeValue::Nothing => "nothing",
                TypeValue::Unbound { is_int: true } => "i32",
                TypeValue::Unbound { is_int: false } => "_",
            },
        }
    }

    /// Rewrite every Infer entry in the arena to its resolved concrete type,
    /// defaulting unconstrained integers to i32. Run once after solving so MIR
    /// and codegen only ever see concrete types.
    pub(crate) fn resolve_all(&mut self) {
        for i in 0..self.types.len() {
            let id = Id::new(i);
            if let Type::Infer { var, span } = self.types[id] {
                let resolved = match self.table.probe_value(var) {
                    TypeValue::I32 => Type::I32 { span },
                    TypeValue::Bool => Type::Bool { span },
                    TypeValue::Nothing => Type::Nothing { span },
                    TypeValue::Unbound { is_int: true } => Type::I32 { span }, // the default
                    TypeValue::Unbound { is_int: false } => Type::Error { span },
                };
                *self.types.get_mut(&id) = resolved;
            }
        }
    }
}

fn prim(ty: Type) -> TypeValue {
    match ty {
        Type::I32 { .. } => TypeValue::I32,
        Type::Bool { .. } => TypeValue::Bool,
        Type::Nothing { .. } => TypeValue::Nothing,
        _ => unreachable!("prim() on a non-concrete type"),
    }
}

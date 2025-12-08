use ena::unify::InPlaceUnificationTable;

use crate::{Ast, Expr, ExprId, TraitId, Type, TypeValue, TypeVar};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Constraint {
    Equal {
        expr: ExprId,
        lhs: Type,
        rhs: Type,
    },
    Trait {
        expr: ExprId,
        trait_id: TraitId,
        args: Vec<Type>,
        output: Type,
    },
}

pub struct TypeInferencer {
    env: HashMap<String, Type>,
    constraints: Vec<Constraint>,
    unification_table: InPlaceUnificationTable<TypeVar>,
}

impl TypeInferencer {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            constraints: Vec::new(),
            unification_table: InPlaceUnificationTable::new(),
        }
    }

    /// generates contraints
    pub fn infer(&mut self, ast: &Ast, expr_id: &ExprId) -> Type {
        let expr = ast.get_expr(expr_id);

        match expr {
            Expr::I32(_) => Type::I32,
            Expr::Var(ident) => self
                .env
                .get(&*ident.value)
                .cloned()
                .unwrap_or_else(|| self.fresh_type()),
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer(ast, lhs);
                let rhs_ty = self.infer(ast, rhs);
                let output_ty = self.fresh_type();

                self.constraints.push(Constraint::Trait {
                    expr: *expr_id,
                    trait_id: op.trait_id(),
                    args: vec![lhs_ty, rhs_ty],
                    output: output_ty.clone(),
                });

                output_ty
            }
        }
    }

    pub fn check(&mut self, ast: &Ast, expr_id: ExprId, expected: Type) {
        let expr = ast.get_expr(&expr_id);

        #[allow(clippy::match_single_binding)]
        match expr {
            _ => {
                let actual = self.infer(ast, &expr_id);
                self.constraints.push(Constraint::Equal {
                    expr: expr_id,
                    lhs: actual,
                    rhs: expected,
                })
            }
        }
    }

    /// normalizes a type
    pub fn normalize(&mut self, ty: &Type) -> Type {
        match ty {
            Type::Unknown(var) => match self.unification_table.probe_value(*var) {
                TypeValue::Bound(ty) => self.normalize(&ty),
                TypeValue::Unbound => ty.clone(),
            },
            Type::Fun { arguments, returns } => Type::Fun {
                arguments: arguments.iter().map(|t| self.normalize(t)).collect(),
                returns: Box::new(self.normalize(returns)),
            },
            _ => ty.clone(),
        }
    }

    fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), String> {
        let t1 = self.normalize(t1);
        let t2 = self.normalize(t2);

        match (&t1, &t2) {
            (Type::I32, Type::I32) | (Type::Bool, Type::Bool) | (Type::Unit, Type::Unit) => Ok(()),

            (Type::Unknown(v1), Type::Unknown(v2)) => {
                self.unification_table
                    .unify_var_var(*v1, *v2)
                    .map_err(|_| "Unification error".to_string())?;
                Ok(())
            }

            (Type::Unknown(var), ty) | (ty, Type::Unknown(var)) => {
                self.unification_table
                    .unify_var_value(*var, TypeValue::Bound(ty.clone()))
                    .map_err(|_| "Unification error".to_string())?;
                Ok(())
            }

            (
                Type::Fun {
                    arguments: a1,
                    returns: r1,
                },
                Type::Fun {
                    arguments: a2,
                    returns: r2,
                },
            ) => {
                if a1.len() != a2.len() {
                    return Err("Function arity mismatch".to_string());
                }
                for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                    self.unify(arg1, arg2)?;
                }
                self.unify(r1, r2)
            }

            _ => Err(format!("Cannot unify {:?} with {:?}", t1, t2)),
        }
    }

    fn fresh_type(&mut self) -> Type {
        Type::Unknown(self.fresh_type_var())
    }

    fn fresh_type_var(&mut self) -> TypeVar {
        self.unification_table.new_key(TypeValue::Unbound)
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    /// solves contraints
    pub fn solve(&mut self, ast: &Ast) -> Result<(), String> {
        let constraints = self.constraints.clone();

        for constraint in constraints {
            match constraint {
                Constraint::Equal { lhs, rhs, .. } => {
                    self.unify(&lhs, &rhs)?;
                }

                Constraint::Trait {
                    trait_id,
                    args,
                    output,
                    ..
                } => {
                    // Normalize args first
                    let normalized_args: Vec<Type> =
                        args.iter().map(|t| self.normalize(t)).collect();

                    // Find trait impl
                    if normalized_args.len() != 2 {
                        return Err("Expected 2 args for binary op trait".to_string());
                    }

                    let impl_def = ast
                        .find_impl(trait_id, &normalized_args[0], &[normalized_args[1].clone()])
                        .ok_or_else(|| {
                            format!(
                                "No impl for trait {:?} on {:?}",
                                trait_id, normalized_args[0]
                            )
                        })?;

                    // Unify output with impl's output type
                    self.unify(&output, &impl_def.output_type)?;
                }
            }
        }

        Ok(())
    }
}

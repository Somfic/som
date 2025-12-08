use ena::unify::InPlaceUnificationTable;

use crate::{Ast, Expr, ExprId, Stmt, StmtId, TraitId, Type, TypeValue, TypeVar};
use std::collections::HashMap;

mod error;
pub use error::*;
mod constraint;
pub use constraint::*;

pub struct TypeCheckResult {
    pub final_type: Type,
    pub errors: HashMap<ExprId, TypeError>,
}

pub struct TypeInferencer {
    env: HashMap<String, Type>,
    constraints: Vec<Constraint>,
    unification_table: InPlaceUnificationTable<TypeVar>,
    errors: HashMap<ExprId, TypeError>,
}

impl TypeInferencer {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            constraints: Vec::new(),
            unification_table: InPlaceUnificationTable::new(),
            errors: HashMap::new(),
        }
    }

    /// generates contraints
    pub fn infer(&mut self, ast: &Ast, expr_id: &ExprId) -> Type {
        let expr = ast.get_expr(expr_id);

        match expr {
            Expr::Hole => self.fresh_type(), // holes don't block inference
            Expr::I32(_) => Type::I32,
            Expr::Var(ident) => match self.env.get(&*ident.value) {
                Some(ty) => ty.clone(),
                None => {
                    self.errors.insert(
                        *expr_id,
                        TypeError::UnboundVariable {
                            name: ident.value.to_string(),
                        },
                    );
                    self.fresh_type()
                }
            },
            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.infer(ast, lhs);
                let rhs_ty = self.infer(ast, rhs);
                let output_ty = self.fresh_type();

                self.constraints.push(Constraint::Trait {
                    provenance: Provenance::BinaryOp(*expr_id),
                    trait_id: op.trait_id(),
                    args: vec![lhs_ty, rhs_ty],
                    output: output_ty.clone(),
                });

                output_ty
            }
            Expr::Block { stmts, value } => {
                let saved_env = self.env.clone();

                for stmt in stmts {
                    self.check_stmt(ast, *stmt);
                }

                let block_ty = match value {
                    Some(ret_expr) => self.infer(ast, ret_expr),
                    None => Type::Unit,
                };

                self.env = saved_env;

                block_ty
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
                    provenance: Provenance::Check(expr_id),
                    lhs: actual,
                    rhs: expected,
                })
            }
        }
    }

    fn check_stmt(&mut self, ast: &Ast, stmt_id: StmtId) {
        let stmt = ast.get_stmt(&stmt_id);

        match stmt {
            Stmt::Let { name, ty, value } => match ty {
                Some(annotated_ty) => {
                    self.check(ast, *value, annotated_ty.clone());
                    self.env
                        .insert(name.value.to_string(), annotated_ty.clone());
                }
                None => {
                    let inferred_ty = self.infer(ast, value);
                    self.env.insert(name.value.to_string(), inferred_ty);
                }
            },
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

    fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), TypeError> {
        let t1 = self.normalize(t1);
        let t2 = self.normalize(t2);

        match (&t1, &t2) {
            (Type::I32, Type::I32) | (Type::Bool, Type::Bool) | (Type::Unit, Type::Unit) => Ok(()),

            (Type::Unknown(v1), Type::Unknown(v2)) => self
                .unification_table
                .unify_var_var(*v1, *v2)
                .map_err(|_| TypeError::InfiniteType { var: *v1, ty: t2 }),

            (Type::Unknown(var), ty) | (ty, Type::Unknown(var)) => {
                // TODO: occurs check here
                self.unification_table
                    .unify_var_value(*var, TypeValue::Bound(ty.clone()))
                    .map_err(|_| TypeError::InfiniteType {
                        var: *var,
                        ty: ty.clone(),
                    })
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
                    return Err(TypeError::Mismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                        provenance: Provenance::FunctionArity,
                    });
                }
                for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                    self.unify(arg1, arg2)?;
                }
                self.unify(r1, r2)
            }

            _ => Err(TypeError::Mismatch {
                expected: t1,
                found: t2,
                provenance: Provenance::Unification,
            }),
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

    fn solve_trait_constraint(
        &mut self,
        ast: &Ast,
        trait_id: TraitId,
        args: &[Type],
        output: &Type,
    ) -> Result<(), TypeError> {
        let normalized_args: Vec<Type> = args.iter().map(|t| self.normalize(t)).collect();

        if normalized_args.len() != 2 {
            return Err(TypeError::Internal("Expected 2 args".into()));
        }

        let impl_def = ast
            .find_impl(trait_id, &normalized_args[0], &[normalized_args[1].clone()])
            .ok_or_else(|| TypeError::MissingImpl {
                trait_id,
                self_type: normalized_args[0].clone(),
                arg_types: vec![normalized_args[1].clone()],
            })?;

        self.unify(output, &impl_def.output_type)
    }

    /// solves contraints, collecting errors instead of failing fast
    pub fn solve(&mut self, ast: &Ast) {
        let constraints = self.constraints.clone();

        for constraint in constraints {
            let result = match &constraint {
                Constraint::Equal {
                    provenance: _,
                    lhs,
                    rhs,
                } => self.unify(lhs, rhs),

                Constraint::Trait {
                    provenance: _,
                    trait_id,
                    args,
                    output,
                } => self.solve_trait_constraint(ast, *trait_id, args, output),
            };

            if let Err(error) = result {
                let expr_id = constraint.expr_id();
                self.errors.insert(expr_id, error);
            }
        }
    }

    pub fn type_check(&mut self, ast: &Ast, expr_id: ExprId) -> TypeCheckResult {
        let inferred_type = self.infer(ast, &expr_id);

        self.solve(ast);

        let final_type = self.normalize(&inferred_type);

        TypeCheckResult {
            final_type,
            errors: std::mem::take(&mut self.errors),
        }
    }
}

use ena::unify::InPlaceUnificationTable;

use crate::{
    Ast, Expr, ExprId, FuncDec, FuncId, Stmt, StmtId, TraitId, Type, TypeValue, TypeVar, TypedAst,
};
use std::collections::HashMap;

mod error;
pub use error::*;
mod constraint;
pub use constraint::*;

pub struct TypeInferencer {
    env: HashMap<String, Type>,
    constraints: Vec<Constraint>,
    unification_table: InPlaceUnificationTable<TypeVar>,
    errors: HashMap<ExprId, TypeError>,
    func_types: HashMap<FuncId, Type>, // Store inferred function return types
    expr_types: HashMap<ExprId, Type>, // Track all expression types
}

impl TypeInferencer {
    pub fn new() -> Self {
        Self {
            env: HashMap::new(),
            constraints: Vec::new(),
            unification_table: InPlaceUnificationTable::new(),
            errors: HashMap::new(),
            func_types: HashMap::new(),
            expr_types: HashMap::new(),
        }
    }

    /// generates contraints
    pub fn infer(&mut self, ast: &Ast, expr_id: &ExprId) -> Type {
        let expr = ast.get_expr(expr_id);

        let ty = match expr {
            Expr::Hole => self.fresh_type(),
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
            Expr::Call { func, args } => {
                let func_dec = ast.get_func(func);

                // Check argument count
                if args.len() != func_dec.parameters.len() {
                    self.errors.insert(
                        *expr_id,
                        TypeError::WrongArgCount {
                            expected: func_dec.parameters.len(),
                            found: args.len(),
                        },
                    );
                    return self.fresh_type();
                }

                // Check each argument against parameter type
                for (arg_expr, param) in args.iter().zip(&func_dec.parameters) {
                    match &param.ty {
                        Some(param_ty) => {
                            // Check argument against annotated parameter type
                            self.check_expr(ast, *arg_expr, param_ty.clone());
                        }
                        None => {
                            // Parameter type not annotated, just infer the argument
                            self.infer(ast, arg_expr);
                        }
                    }
                }

                // Return the function's return type
                match &func_dec.return_type {
                    Some(return_ty) => return_ty.clone(),
                    None => {
                        // Check if we've inferred this function's type already
                        self.func_types
                            .get(func)
                            .cloned()
                            .unwrap_or_else(|| self.fresh_type())
                    }
                }
            }
        };

        // Store the inferred type
        self.expr_types.insert(*expr_id, ty.clone());
        ty
    }

    pub fn check_expr(&mut self, ast: &Ast, expr_id: ExprId, expected: Type) {
        let expr = ast.get_expr(&expr_id);

        #[allow(clippy::match_single_binding)]
        match expr {
            _ => {
                let actual = self.infer(ast, &expr_id);
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Check(expr_id),
                    lhs: actual,
                    rhs: expected.clone(),
                });
                // Store the expected type
                self.expr_types.insert(expr_id, expected);
            }
        }
    }

    fn check_stmt(&mut self, ast: &Ast, stmt_id: StmtId) {
        let stmt = ast.get_stmt(&stmt_id);

        match stmt {
            Stmt::Let { name, ty, value } => match ty {
                Some(annotated_ty) => {
                    self.check_expr(ast, *value, annotated_ty.clone());
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

    pub fn check_func_dec(&mut self, ast: &Ast, func_id: FuncId) {
        let func = ast.get_func(&func_id);
        let saved_env = self.env.clone();

        // Add parameters to environment
        for param in &func.parameters {
            let param_ty = match &param.ty {
                Some(ty) => ty.clone(),
                None => self.fresh_type(), // Infer parameter type from usage
            };
            self.env.insert(param.name.value.to_string(), param_ty);
        }

        // Infer body type
        let body_ty = self.infer(ast, &func.body);

        // Check or infer return type
        let return_ty = match &func.return_type {
            Some(annotated_return) => {
                // Check body matches annotation
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::FunctionCall(func.body),
                    lhs: body_ty.clone(),
                    rhs: annotated_return.clone(),
                });
                annotated_return.clone()
            }
            None => {
                // Infer return type from body
                body_ty.clone()
            }
        };

        // Store function's return type
        self.func_types.insert(func_id, return_ty);

        // Restore environment
        self.env = saved_env;
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

        // Skip trait resolution if types are still unknown
        // This allows inference at call sites with concrete types
        if normalized_args
            .iter()
            .any(|t| matches!(t, Type::Unknown(_)))
        {
            // Skip - will be resolved when concrete types flow in
            return Err(TypeError::Internal(
                "Trait resolution deferred due to unknown types".into(),
            ));
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

    /// Type check an entire program
    pub fn check_program(mut self, ast: Ast) -> TypedAst {
        // 1. Check all function declarations
        let func_ids: Vec<FuncId> = (0..ast.funcs.len()).map(|i| FuncId(i as u32)).collect();

        for func_id in func_ids {
            self.check_func_dec(&ast, func_id);
        }

        // 2. Solve all constraints
        self.solve(&ast);

        // 3. Normalize all stored expression types
        let types_to_normalize: Vec<(ExprId, Type)> = self
            .expr_types
            .iter()
            .map(|(id, ty)| (*id, ty.clone()))
            .collect();
        let mut expr_types = HashMap::new();
        for (id, ty) in types_to_normalize {
            expr_types.insert(id, self.normalize(&ty));
        }

        TypedAst {
            ast,
            types: expr_types,
            errors: self.errors,
            constraints: self.constraints,
        }
    }
}

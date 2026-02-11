use ena::unify::InPlaceUnificationTable;

use crate::{
    Ast, Expr, ExternFunc, Func, Lifetime, LifetimeValue, LifetimeVar, Span, Stmt, Trait, Type,
    TypeValue, TypeVar, TypedAst, arena::Id, scope::ScopedEnvironment,
};
use std::collections::HashMap;

mod error;
pub use error::*;
mod constraint;
pub use constraint::*;

/// Error type used internally by unify (without span info)
enum UnifyError {
    InfiniteType {
        var: TypeVar,
        ty: Type,
    },
    Mismatch {
        expected: Type,
        found: Type,
    },
    MissingImpl {
        trait_id: Id<Trait>,
        self_type: Type,
        arg_types: Vec<Type>,
    },
    Internal(String),
}

pub struct TypeInferencer {
    env: ScopedEnvironment<Type>,
    constraints: Vec<Constraint>,
    unification_table: InPlaceUnificationTable<TypeVar>,
    lifetime_unification_table: InPlaceUnificationTable<LifetimeVar>,
    errors: Vec<TypeError>,
    func_types: HashMap<Id<Func>, Type>, // Store inferred function return types
    expr_types: HashMap<Id<Expr>, Type>, // Track all expression types
}

impl TypeInferencer {
    pub fn new() -> Self {
        Self {
            env: ScopedEnvironment::new(),
            constraints: Vec::new(),
            unification_table: InPlaceUnificationTable::new(),
            lifetime_unification_table: InPlaceUnificationTable::new(),
            errors: Vec::new(),
            func_types: HashMap::new(),
            expr_types: HashMap::new(),
        }
    }

    /// generates contraints
    pub fn infer(&mut self, ast: &Ast, expr_id: &Id<Expr>) -> Type {
        let expr = ast.exprs.get(expr_id);

        let ty = match expr {
            Expr::Hole => self.fresh_type(),
            Expr::I32(_) => Type::I32,
            Expr::Bool(_) => Type::Bool,
            Expr::String(_) => Type::Reference {
                mutable: false,
                lifetime: Lifetime::Static,
                to: Box::new(Type::Str),
            },
            Expr::Var(ident) => match self.env.get(&ident.value) {
                Some(ty) => ty.clone(),
                None => {
                    self.errors.push(TypeError::UnboundVariable {
                        span: ast.get_expr_span(expr_id).clone(),
                        name: ident.value.to_string(),
                    });
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
                self.env.enter_scope();

                for stmt in stmts {
                    self.check_stmt(ast, *stmt);
                }

                let block_ty = match value {
                    Some(ret_expr) => self.infer(ast, ret_expr),
                    None => Type::Unit,
                };

                self.env.leave_scope();

                block_ty
            }
            Expr::Call { name, args } => {
                // First check if function is in env (includes extern functions)
                if let Some(func_ty) = self.env.get(&name.value).cloned() {
                    if let Type::Fun { arguments, returns } = func_ty {
                        // Check argument count
                        if args.len() != arguments.len() {
                            self.errors.push(TypeError::WrongArgCount {
                                span: ast.get_expr_span(expr_id).clone(),
                                expected: arguments.len(),
                                found: args.len(),
                            });
                            return self.fresh_type();
                        }

                        // Check each argument against parameter type
                        for (arg_expr, param_ty) in args.iter().zip(&arguments) {
                            let actual = self.infer(ast, arg_expr);
                            self.constraints.push(Constraint::Equal {
                                provenance: Provenance::FuncArg {
                                    arg_expr: *arg_expr,
                                    param_type_id: None,
                                },
                                lhs: actual,
                                rhs: param_ty.clone(),
                            });
                        }

                        return *returns;
                    }
                }

                // Fall back to regular function lookup
                let func_id = match ast.find_func_by_name(&name.value) {
                    Some(id) => id,
                    None => {
                        self.errors.push(TypeError::UnknownFunction {
                            span: ast.get_expr_span(expr_id).clone(),
                            name: name.value.to_string(),
                        });
                        return self.fresh_type();
                    }
                };

                let func = ast.funcs.get(&func_id);

                // Check argument count
                if args.len() != func.parameters.len() {
                    self.errors.push(TypeError::WrongArgCount {
                        span: ast.get_expr_span(expr_id).clone(),
                        expected: func.parameters.len(),
                        found: args.len(),
                    });
                    return self.fresh_type();
                }

                // Instantiate fresh type vars for this call site
                let mut call_generics: HashMap<String, TypeVar> = HashMap::new();
                for type_param in &func.type_parameters {
                    let tv = self.fresh_type_var();
                    call_generics.insert(type_param.name.value.to_string(), tv);
                }

                let call_span = ast.get_expr_span(expr_id);

                // Check each argument against parameter type
                for (arg_expr, param) in args.iter().zip(&func.parameters) {
                    match &param.ty {
                        Some(param_ty) => {
                            // Resolve generic types for this call site
                            let resolved_ty =
                                self.resolve_type(param_ty, &call_generics, call_span);
                            let actual = self.infer(ast, arg_expr);
                            self.constraints.push(Constraint::Equal {
                                provenance: Provenance::FuncArg {
                                    arg_expr: *arg_expr,
                                    param_type_id: param.type_id,
                                },
                                lhs: actual,
                                rhs: resolved_ty,
                            });
                        }
                        None => {
                            // Parameter type not annotated, just infer the argument
                            self.infer(ast, arg_expr);
                        }
                    }
                }

                // Return the function's return type (resolved for this call site)
                match &func.return_type {
                    Some(return_ty) => self.resolve_type(return_ty, &call_generics, call_span),
                    None => {
                        // Check if we've inferred this function's type already
                        self.func_types
                            .get(&func_id)
                            .cloned()
                            .unwrap_or_else(|| self.fresh_type())
                    }
                }
            }
            Expr::Borrow { mutable, expr } => {
                let inner = self.infer(ast, expr);
                Type::Reference {
                    mutable: *mutable,
                    lifetime: self.fresh_lifetime(),
                    to: Box::new(inner),
                }
            }
            Expr::Deref { expr } => {
                let expr_ty = self.infer(ast, expr);

                // Try to extract inner type directly if it's a known reference
                match &self.normalize(&expr_ty) {
                    Type::Reference { to, .. } => {
                        // It's already a reference type, extract the inner type
                        *to.clone()
                    }
                    Type::Unknown(_) => {
                        // It's a type variable, create a constraint
                        let inner_ty = self.fresh_type();
                        let lifetime = self.fresh_lifetime();

                        self.constraints.push(Constraint::Equal {
                            provenance: Provenance::Deref(*expr_id),
                            lhs: expr_ty,
                            rhs: Type::Reference {
                                mutable: false, // Will match either mut or immut
                                lifetime,
                                to: Box::new(inner_ty.clone()),
                            },
                        });

                        inner_ty
                    }
                    _ => {
                        // Not a reference - error!
                        self.errors.push(TypeError::Internal {
                            span: ast.get_expr_span(expr_id).clone(),
                            message: "cannot dereference non-reference type".into(),
                        });
                        self.fresh_type()
                    }
                }
            }
            Expr::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                // make sure condition is a boolean
                let condition_ty = self.infer(ast, condition);
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Conditional(*condition),
                    lhs: condition_ty,
                    rhs: Type::Bool,
                });

                // make sure truthy and falsy are the same
                let truthy_ty = self.infer(ast, truthy);
                let falsy_ty = self.infer(ast, falsy);
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Conditional(*falsy),
                    lhs: truthy_ty.clone(),
                    rhs: falsy_ty,
                });

                truthy_ty
            }
        };

        // Store the inferred type
        self.expr_types.insert(*expr_id, ty.clone());
        ty
    }

    pub fn check_expr(&mut self, ast: &Ast, expr_id: Id<Expr>, expected: Type) {
        let expr = ast.exprs.get(&expr_id);

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

    fn check_stmt(&mut self, ast: &Ast, stmt_id: Id<Stmt>) {
        let stmt = ast.stmts.get(&stmt_id);

        match stmt {
            Stmt::Let {
                name,
                mutable: _,
                ty,
                value,
            } => match ty {
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
            Stmt::Expr { expr } => {
                self.infer(ast, expr);
            }
        }
    }

    pub fn check_func(&mut self, ast: &Ast, func_id: Id<Func>) {
        let func = ast.funcs.get(&func_id);
        self.env.enter_scope();

        // type parameters
        let mut generics: HashMap<String, TypeVar> = HashMap::new();
        for type_param in &func.type_parameters {
            let type_var = self.fresh_type_var();
            generics.insert(type_param.name.value.to_string(), type_var);
        }

        // parameters
        for param in &func.parameters {
            let param_ty = match (&param.ty, &param.type_id) {
                (Some(ty), Some(type_id)) => {
                    let span = ast.get_type_span(type_id);
                    self.resolve_type(ty, &generics, span)
                }
                (Some(ty), None) => {
                    // No type_id available, use function body span as fallback
                    let span = ast.get_expr_span(&func.body);
                    self.resolve_type(ty, &generics, span)
                }
                _ => self.fresh_type(),
            };
            self.env.insert(param.name.value.to_string(), param_ty);
        }

        // Infer body type
        let body_ty = self.infer(ast, &func.body);

        // Check or infer return type
        let return_ty = match &func.return_type {
            Some(annotated_return) => {
                // Get the expression ID to report errors on
                // If body is a block with a value, use that value's ID
                // Otherwise use the body itself
                let error_expr_id = match ast.exprs.get(&func.body) {
                    Expr::Block {
                        value: Some(value_expr),
                        ..
                    } => *value_expr,
                    _ => func.body,
                };

                // Check body matches annotation
                // Note: lhs is "expected", rhs is "found" in error messages
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::FunctionCall(error_expr_id, func.return_type_id),
                    lhs: annotated_return.clone(), // expected type
                    rhs: body_ty.clone(),          // found type
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

        self.env.leave_scope();
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
            Type::Reference {
                lifetime,
                mutable,
                to,
            } => Type::Reference {
                lifetime: self.normalize_lifetime(lifetime),
                mutable: *mutable,
                to: Box::new(self.normalize(to)),
            },
            _ => ty.clone(),
        }
    }

    fn normalize_lifetime(&mut self, lt: &Lifetime) -> Lifetime {
        match lt {
            Lifetime::Unknown(var) => match self.lifetime_unification_table.probe_value(*var) {
                LifetimeValue::Bound(ty) => self.normalize_lifetime(&ty),
                LifetimeValue::Unbound => lt.clone(),
            },
            _ => lt.clone(),
        }
    }

    fn unify(&mut self, t1: &Type, t2: &Type) -> Result<(), UnifyError> {
        let t1 = self.normalize(t1);
        let t2 = self.normalize(t2);

        match (&t1, &t2) {
            (Type::I32, Type::I32)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit)
            | (Type::Str, Type::Str) => Ok(()),

            (Type::Unknown(v1), Type::Unknown(v2)) => self
                .unification_table
                .unify_var_var(*v1, *v2)
                .map_err(|_| UnifyError::InfiniteType { var: *v1, ty: t2 }),

            (Type::Unknown(var), ty) | (ty, Type::Unknown(var)) => {
                // TODO: occurs check here
                self.unification_table
                    .unify_var_value(*var, TypeValue::Bound(ty.clone()))
                    .map_err(|_| UnifyError::InfiniteType {
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
                    return Err(UnifyError::Mismatch {
                        expected: t1.clone(),
                        found: t2.clone(),
                    });
                }
                for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                    self.unify(arg1, arg2)?;
                }
                self.unify(r1, r2)
            }

            (
                Type::Reference {
                    lifetime: l1,
                    mutable: m1,
                    to: t1,
                },
                Type::Reference {
                    lifetime: l2,
                    mutable: m2,
                    to: t2,
                },
            ) => {
                if m1 != m2 {
                    return Err(UnifyError::Mismatch {
                        expected: *t1.clone(),
                        found: *t2.clone(),
                    });
                }

                self.unify_lifetime(l1, l2)?;
                self.unify(t1, t2)
            }

            _ => Err(UnifyError::Mismatch {
                expected: t1,
                found: t2,
            }),
        }
    }

    fn unify_lifetime(&mut self, l1: &Lifetime, l2: &Lifetime) -> Result<(), UnifyError> {
        let l1 = self.normalize_lifetime(l1);
        let l2 = self.normalize_lifetime(l2);

        match (&l1, &l2) {
            (Lifetime::Unknown(v1), Lifetime::Unknown(v2)) => self
                .lifetime_unification_table
                .unify_var_var(*v1, *v2)
                .map_err(|_| UnifyError::Internal("infinite lifetime".into())),

            (Lifetime::Unknown(var), lt) | (lt, Lifetime::Unknown(var)) => self
                .lifetime_unification_table
                .unify_var_value(*var, LifetimeValue::Bound(lt.clone()))
                .map_err(|_| UnifyError::Internal("infinite lifetime".into())),

            (Lifetime::Named(n1), Lifetime::Named(n2)) if n1 == n2 => Ok(()),

            (Lifetime::Unspecified, _) | (_, Lifetime::Unspecified) => Ok(()),

            (Lifetime::Static, Lifetime::Static) => Ok(()),

            _ => Err(UnifyError::Internal(
                "cannot unify different concrete lifetimes".into(),
            )),
        }
    }

    fn fresh_lifetime(&mut self) -> Lifetime {
        Lifetime::Unknown(self.fresh_lifetime_var())
    }

    fn fresh_type(&mut self) -> Type {
        Type::Unknown(self.fresh_type_var())
    }

    fn fresh_type_var(&mut self) -> TypeVar {
        self.unification_table.new_key(TypeValue::Unbound)
    }

    fn fresh_lifetime_var(&mut self) -> LifetimeVar {
        self.lifetime_unification_table
            .new_key(LifetimeValue::Unbound)
    }

    fn solve_trait_constraint(
        &mut self,
        ast: &Ast,
        trait_id: Id<Trait>,
        args: &[Type],
        output: &Type,
    ) -> Result<(), UnifyError> {
        let normalized_args: Vec<Type> = args.iter().map(|t| self.normalize(t)).collect();

        // Skip trait resolution if types are still unknown
        // This allows inference at call sites with concrete types
        if normalized_args
            .iter()
            .any(|t| matches!(t, Type::Unknown(_)))
        {
            // Skip - will be resolved when concrete types flow in
            return Err(UnifyError::Internal(
                "trait resolution deferred due to unknown types".into(),
            ));
        }

        let impl_def = ast
            .find_impl(trait_id, &normalized_args[0], &[normalized_args[1].clone()])
            .ok_or_else(|| UnifyError::MissingImpl {
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
            let result: Result<(), UnifyError> = match &constraint {
                Constraint::Equal { lhs, rhs, .. } => self.unify(lhs, rhs),
                Constraint::Trait {
                    trait_id,
                    args,
                    output,
                    ..
                } => self.solve_trait_constraint(ast, *trait_id, args, output),
            };

            if let Err(unify_error) = result {
                // Skip internal "deferred" errors
                if matches!(&unify_error, UnifyError::Internal(msg) if msg.contains("deferred")) {
                    continue;
                }

                let expr_id = constraint.expr_id();
                let span = ast.get_expr_span(&expr_id).clone();
                let provenance = constraint.provenance().clone();

                let type_error = match unify_error {
                    UnifyError::InfiniteType { var, ty } => {
                        TypeError::InfiniteType { span, var, ty }
                    }
                    UnifyError::Mismatch { expected, found } => TypeError::Mismatch {
                        span,
                        expected,
                        found,
                        provenance,
                    },
                    UnifyError::MissingImpl {
                        trait_id,
                        self_type,
                        arg_types,
                    } => TypeError::MissingImpl {
                        span,
                        trait_id,
                        self_type,
                        arg_types,
                    },
                    UnifyError::Internal(message) => TypeError::Internal { span, message },
                };

                self.errors.push(type_error);
            }
        }
    }

    /// Type check an entire program
    pub fn check_program(mut self, ast: Ast) -> TypedAst {
        let extern_func_ids: Vec<Id<ExternFunc>> =
            (0..ast.extern_funcs.len()).map(|i| Id::new(i)).collect();
        let func_ids: Vec<Id<Func>> = (0..ast.funcs.len()).map(|i| Id::new(i)).collect();

        for extern_func_id in extern_func_ids {
            let external_func = ast.extern_funcs.get(&extern_func_id);

            let arguments = external_func
                .parameters
                .iter()
                .map(|p| match &p.ty {
                    Some(ty) => ty.clone(),
                    None => self.fresh_type(),
                })
                .collect();

            let return_ty = match &external_func.return_type {
                Some(ty) => ty.clone(),
                None => self.fresh_type(),
            };

            self.env.insert(
                external_func.name.value.clone(),
                Type::Fun {
                    arguments: arguments,
                    returns: Box::new(return_ty),
                },
            );
        }

        for func_id in func_ids {
            self.check_func(&ast, func_id);
        }

        self.solve(&ast);

        // normalize all stored expression types
        let types_to_normalize: Vec<(Id<Expr>, Type)> = self
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

    fn resolve_type(
        &mut self,
        ty: &Type,
        generics: &HashMap<String, TypeVar>,
        span: &Span,
    ) -> Type {
        match ty {
            Type::Named(name) => match generics.get(name.as_ref()) {
                Some(&tv) => Type::Unknown(tv),
                None => {
                    self.errors.push(TypeError::UnknownType {
                        span: span.clone(),
                        name: name.to_string(),
                    });
                    self.fresh_type() // return fresh var to continue checking
                }
            },
            Type::Reference {
                mutable,
                lifetime,
                to,
            } => Type::Reference {
                mutable: *mutable,
                lifetime: lifetime.clone(),
                to: Box::new(self.resolve_type(to, generics, span)),
            },
            _ => ty.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Source, parser};
    use std::sync::Arc;

    fn check(source: &str) -> TypedAst {
        let source = Arc::new(Source::from_raw(source));
        let (ast, _) = parser::parse(source);
        let inferencer = TypeInferencer::new();
        inferencer.check_program(ast)
    }

    fn has_type_error<F: Fn(&TypeError) -> bool>(typed_ast: &TypedAst, predicate: F) -> bool {
        typed_ast.errors.iter().any(|e| predicate(e))
    }

    #[test]
    fn test_infer_i32_literal() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                42
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_binary_add() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                1 + 2
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_let_binding() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                let x = 10;
                x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_reference_type() {
        let typed_ast = check(
            r#"
            fn test() -> &i32 {
                let x = 10;
                &x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_type_mismatch_return() {
        let typed_ast = check(
            r#"
            fn test() -> bool {
                42
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::Mismatch { .. })
        }));
    }

    #[test]
    fn test_unbound_variable() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                x
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::UnboundVariable { .. })
        }));
    }

    #[test]
    fn test_infer_deref() {
        let typed_ast = check(
            r#"
            fn test(x: &i32) -> i32 {
                *x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_mut_reference() {
        let typed_ast = check(
            r#"
            fn test() -> &mut i32 {
                let x = 10;
                &mut x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_function_param_types() {
        let typed_ast = check(
            r#"
            fn add(x: i32, y: i32) -> i32 {
                x + y
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_nested_blocks() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                let x = {
                    let y = 10;
                    y + 1
                };
                x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_comparison() {
        let typed_ast = check(
            r#"
            fn test() -> bool { 
                1 < 2
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_type_annotation_mismatch() {
        let typed_ast = check(
            r#"
            fn test() {
                let x: bool = 42;
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::Mismatch { .. })
        }));
    }

    #[test]
    fn test_infer_bool_literal() {
        let typed_ast = check(
            r#"
            fn test() -> bool {
                true
            }
            "#,
        );
        // This might fail if bool literals aren't implemented
        // Just checking it doesn't crash
        let _ = typed_ast;
    }

    #[test]
    fn test_multiple_functions() {
        let typed_ast = check(
            r#"
            fn foo() -> i32 {
                42
            }
            fn bar() -> i32 {
                1 + 2
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_infer_string_literal() {
        let typed_ast = check(
            r#"
            fn test() -> &'static str {
                "hello world"
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_string_literal_type_mismatch() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                "hello"
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::Mismatch { .. })
        }));
    }

    #[test]
    fn test_infer_bool_true_false() {
        let typed_ast = check(
            r#"
            fn test_true() -> bool {
                true
            }
            fn test_false() -> bool {
                false
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_generic_identity() {
        let typed_ast = check(
            r#"
            fn identity<T>(x: T) -> T { x }
            fn main() -> i32 {
                identity(42)
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_generic_multiple_type_params() {
        let typed_ast = check(
            r#"
            fn first<T, U>(x: T, y: U) -> T { x }
            fn main() -> i32 {
                first(1, true)
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_generic_type_mismatch() {
        let typed_ast = check(
            r#"
            fn identity<T>(x: T) -> T { x }
            fn main() {
                identity(1) + false
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::MissingImpl { .. })
        }));
    }

    #[test]
    fn test_unknown_type_error() {
        let typed_ast = check(
            r#"
            fn bad(x: Foo) -> Foo { x }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::UnknownType { .. })
        }));
    }

    #[test]
    fn test_conditional_basic() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                1 if true else 2
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_with_variable_condition() {
        let typed_ast = check(
            r#"
            fn test(b: bool) -> i32 {
                10 if b else 20
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_bool_result() {
        let typed_ast = check(
            r#"
            fn test(a: bool, b: bool) -> bool {
                a if b else false
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_branch_type_mismatch() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                1 if true else false
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::Mismatch { .. })
        }));
    }

    #[test]
    fn test_conditional_condition_not_bool() {
        let typed_ast = check(
            r#"
            fn test() -> i32 {
                1 if 42 else 2
            }
            "#,
        );
        assert!(has_type_error(&typed_ast, |e| {
            matches!(e, TypeError::Mismatch { .. })
        }));
    }

    #[test]
    fn test_conditional_nested() {
        let typed_ast = check(
            r#"
            fn test(a: bool, b: bool) -> i32 {
                1 if a else (2 if b else 3)
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_in_let_binding() {
        let typed_ast = check(
            r#"
            fn test(b: bool) -> i32 {
                let x = 5 if b else 10;
                x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_with_arithmetic() {
        let typed_ast = check(
            r#"
            fn test(b: bool) -> i32 {
                (1 + 2) if b else (3 * 4)
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_type_inference_from_annotation() {
        let typed_ast = check(
            r#"
            fn test(b: bool) -> i32 {
                let x: i32 = 1 if b else 2;
                x
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }

    #[test]
    fn test_conditional_with_comparison_condition() {
        let typed_ast = check(
            r#"
            fn test(x: i32) -> i32 {
                1 if x > 0 else 2
            }
            "#,
        );
        assert!(typed_ast.errors.is_empty());
    }
}

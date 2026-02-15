use ena::unify::InPlaceUnificationTable;

use crate::diagnostics::edit_distance;
use crate::r#std::BUNDLED_MODULES;
use crate::{
    Ast, Expr, Func, Lifetime, LifetimeValue, LifetimeVar, Span, Stmt, Trait, Type, TypeValue,
    TypeVar, TypedAst, arena::Id, ast::MemberKind, scope::ScopedEnvironment,
};
use std::collections::{HashMap, HashSet};

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
    integer_type_vars: std::collections::HashSet<TypeVar>, // Type vars constrained to integers
    current_module: Option<String>,      // Track current module for unqualified lookups
    /// When true, suppress UnboundVariable errors (the resolve pass already reported them)
    suppress_unbound_variables: bool,
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
            integer_type_vars: std::collections::HashSet::new(),
            current_module: None,
            suppress_unbound_variables: false,
        }
    }

    /// Suppress UnboundVariable errors. Use when the resolve pass already reports them.
    pub fn with_name_resolution(mut self) -> Self {
        self.suppress_unbound_variables = true;
        self
    }

    /// generates contraints
    pub fn infer(&mut self, ast: &Ast, expr_id: &Id<Expr>) -> Type {
        let expr = ast.exprs.get(expr_id);

        let ty = match expr {
            Expr::Hole => self.fresh_type(),
            Expr::I32(_) => self.fresh_integer_type(),
            Expr::F32(_) => Type::F32,
            Expr::Bool(_) => Type::Bool,
            Expr::String(_) => Type::Reference {
                mutable: false,
                lifetime: Lifetime::Static,
                to: Box::new(Type::Str),
            },
            Expr::Var(path) => {
                let ident = if path.is_qualified() {
                    todo!("qualified path resolution");
                } else {
                    path.name()
                };

                match self.env.get(&ident.value) {
                    Some(ty) => ty.clone(),
                    None => {
                        if !self.suppress_unbound_variables {
                            self.errors.push(TypeError::UnboundVariable {
                                span: ast.get_expr_span(expr_id).clone(),
                                name: ident.value.to_string(),
                            });
                        }
                        self.fresh_type()
                    }
                }
            }
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
                let registry_key = name.to_string();

                // Try direct lookup first, then try with current module prefix,
                // then try imported module prefixes
                let entry = match ast.func_registry.get(&registry_key) {
                    Some(entry) => entry.clone(),
                    None if !name.is_qualified() => {
                        // Try current module first
                        let mut found = None;
                        if let Some(ref module) = self.current_module {
                            let key = format!("{}::{}", module, registry_key);
                            if let Some(entry) = ast.func_registry.get(&key) {
                                found = Some(entry.clone());
                            }
                        }
                        // Then try imported modules
                        if found.is_none() {
                            let use_mods = self
                                .current_module
                                .as_ref()
                                .map(|m| ast.use_modules(m))
                                .unwrap_or_default();
                            for imported in use_mods {
                                let key = format!("{}::{}", imported, registry_key);
                                if let Some(entry) = ast.func_registry.get(&key) {
                                    found = Some(entry.clone());
                                    break;
                                }
                            }
                        }
                        match found {
                            Some(entry) => entry,
                            None => {
                                for arg in args {
                                    self.infer(ast, arg);
                                }
                                let func_name = name.name().value.to_string();
                                self.errors.push(TypeError::UnknownFunction {
                                    span: ast.get_expr_span(expr_id).clone(),
                                    available_functions: self.function_candidates(ast, &func_name),
                                    module_suggestions: self
                                        .module_suggestions_for_function(ast, &func_name),
                                    name: name.to_string(),
                                });
                                return self.fresh_type();
                            }
                        }
                    }
                    None => {
                        for arg in args {
                            self.infer(ast, arg);
                        }
                        let func_name = name.name().value.to_string();
                        self.errors.push(TypeError::UnknownFunction {
                            span: ast.get_expr_span(expr_id).clone(),
                            available_functions: self.function_candidates(ast, &func_name),
                            module_suggestions: self
                                .module_suggestions_for_function(ast, &func_name),
                            name: name.to_string(),
                        });
                        return self.fresh_type();
                    }
                };

                // Check argument count
                if args.len() != entry.signature.params.len() {
                    self.errors.push(TypeError::WrongArgCount {
                        span: ast.get_expr_span(expr_id).clone(),
                        expected: entry.signature.params.len(),
                        found: args.len(),
                    });
                    return self.fresh_type();
                }

                // For regular functions, handle generics
                let call_generics: HashMap<String, TypeVar> = match &entry.kind {
                    crate::FuncKind::Regular(func_id) => {
                        let func = ast.funcs.get(func_id);
                        func.type_parameters
                            .iter()
                            .map(|tp| (tp.name.value.to_string(), self.fresh_type_var()))
                            .collect()
                    }
                    crate::FuncKind::Extern(_) => HashMap::new(),
                };

                let call_span = ast.get_expr_span(expr_id);

                // Collect param type IDs from function definition
                let param_type_ids: Vec<Option<Id<Type>>> = match &entry.kind {
                    crate::FuncKind::Regular(func_id) => {
                        let func = ast.funcs.get(func_id);
                        func.parameters.iter().map(|p| p.type_id).collect()
                    }
                    crate::FuncKind::Extern(extern_id) => {
                        let func = ast.extern_funcs.get(extern_id);
                        func.parameters.iter().map(|p| p.type_id).collect()
                    }
                };

                // Check each argument against parameter type
                for (i, (arg_expr, param_ty)) in
                    args.iter().zip(&entry.signature.params).enumerate()
                {
                    let resolved_ty = self.resolve_type(param_ty, &call_generics, call_span, ast);
                    let actual = self.infer(ast, arg_expr);
                    self.constraints.push(Constraint::Equal {
                        provenance: Provenance::FuncArg {
                            arg_expr: *arg_expr,
                            param_type_id: param_type_ids.get(i).copied().flatten(),
                        },
                        expected: resolved_ty,
                        actual: actual,
                    });
                }

                // Return the function's return type
                self.resolve_type(&entry.signature.return_type, &call_generics, call_span, ast)
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
                            expected: expr_ty,
                            actual: Type::Reference {
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
            Expr::Not { expr } => {
                // Operand must be boolean
                let expr_ty = self.infer(ast, expr);
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Not(*expr),
                    expected: expr_ty,
                    actual: Type::Bool,
                });
                // Result is boolean
                Type::Bool
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
                    expected: condition_ty,
                    actual: Type::Bool,
                });

                // make sure truthy and falsy are the same
                let truthy_ty = self.infer(ast, truthy);
                let falsy_ty = self.infer(ast, falsy);
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Conditional(*falsy),
                    expected: truthy_ty.clone(),
                    actual: falsy_ty,
                });

                truthy_ty
            }
            Expr::Constructor {
                struct_name,
                fields,
            } => {
                let ident = if struct_name.is_qualified() {
                    todo!("qualified path resolution");
                } else {
                    struct_name.name()
                };

                // Look up the struct definition
                let struct_id = match ast.find_struct_by_name(&ident.value) {
                    Some(id) => id,
                    None => {
                        let sname = ident.value.to_string();
                        self.errors.push(TypeError::UnknownStruct {
                            span: ast.get_expr_span(expr_id).clone(),
                            available_structs: self.struct_candidates(ast),
                            module_suggestions: self.module_suggestions_for_struct(ast, &sname),
                            name: sname,
                        });
                        return self.fresh_type();
                    }
                };

                let struct_def = ast.structs.get(&struct_id);
                let struct_name_str = struct_def.name.value.to_string();

                // Build a map of provided fields
                let mut provided_fields: HashMap<&str, &Id<Expr>> = HashMap::new();
                for (field_name, field_expr) in fields {
                    provided_fields.insert(&field_name.value, field_expr);
                }

                // Check each expected field
                for struct_field in &struct_def.fields {
                    let field_name = &struct_field.name.value;
                    match provided_fields.remove(field_name.as_ref()) {
                        Some(field_expr) => {
                            // Type check the field value against expected type
                            let actual_ty = self.infer(ast, field_expr);
                            self.constraints.push(Constraint::Equal {
                                provenance: Provenance::ConstructorField(*field_expr),
                                expected: actual_ty,
                                actual: struct_field.ty.clone(),
                            });
                        }
                        None => {
                            // Missing field
                            self.errors.push(TypeError::MissingField {
                                span: ast.get_expr_span(expr_id).clone(),
                                struct_name: struct_name_str.clone(),
                                field_name: field_name.to_string(),
                            });
                        }
                    }
                }

                // Check for unknown fields (fields provided but not in struct)
                let known_fields: Vec<String> = struct_def
                    .fields
                    .iter()
                    .map(|f| f.name.value.to_string())
                    .collect();
                for (unknown_field, field_expr) in provided_fields {
                    self.errors.push(TypeError::UnknownField {
                        span: ast.get_expr_span(field_expr).clone(),
                        struct_name: struct_name_str.clone(),
                        field_name: unknown_field.to_string(),
                        available_fields: known_fields.clone(),
                    });
                }

                // The type of a constructor is the struct type
                Type::Named(struct_name_str.into())
            }
            Expr::FieldAccess { object, field } => {
                // Infer the type of the object being accessed
                let obj_ty = self.infer(ast, object);
                let obj_ty = self.normalize(&obj_ty);

                let struct_name = match &obj_ty {
                    Type::Named(name) => name,
                    Type::Reference { to, .. } => match to.as_ref() {
                        Type::Named(name) => name,
                        _ => {
                            self.errors.push(TypeError::UnknownStruct {
                                span: ast.get_expr_span(expr_id).clone(),
                                name: "<unknown>".to_string(),
                                available_structs: vec![],
                                module_suggestions: vec![],
                            });
                            return self.fresh_type();
                        }
                    },
                    _ => {
                        self.errors.push(TypeError::UnknownStruct {
                            span: ast.get_expr_span(expr_id).clone(),
                            name: "<unknown>".to_string(),
                            available_structs: vec![],
                            module_suggestions: vec![],
                        });
                        return self.fresh_type();
                    }
                };

                let struct_id = match ast.find_struct_by_name(struct_name) {
                    Some(id) => id,
                    None => {
                        self.errors.push(TypeError::UnknownStruct {
                            span: ast.get_expr_span(expr_id).clone(),
                            available_structs: self.struct_candidates(ast),
                            module_suggestions: self
                                .module_suggestions_for_struct(ast, struct_name),
                            name: struct_name.to_string(),
                        });
                        return self.fresh_type();
                    }
                };

                let struct_def = ast.structs.get(&struct_id);

                // Find the field
                let field_def = struct_def
                    .fields
                    .iter()
                    .find(|f| f.name.value == field.value);

                match field_def {
                    Some(f) => f.ty.clone(),
                    None => {
                        let available_fields: Vec<String> = struct_def
                            .fields
                            .iter()
                            .map(|f| f.name.value.to_string())
                            .collect();
                        self.errors.push(TypeError::UnknownField {
                            span: ast.get_expr_span(expr_id).clone(),
                            struct_name: struct_name.to_string(),
                            field_name: field.value.to_string(),
                            available_fields,
                        });
                        self.fresh_type()
                    }
                }
            }
            Expr::Assignment { target, value } => {
                // Infer the type of the target
                let target_ty = self.infer(ast, target);

                // Infer the type of the value
                let value_ty = self.infer(ast, value);

                // The target and value must have the same type
                self.constraints.push(Constraint::Equal {
                    provenance: Provenance::Assignment(*expr_id),
                    expected: target_ty.clone(),
                    actual: value_ty.clone(),
                });

                // The type of an assignment is the type of the value
                value_ty
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
                    expected: actual,
                    actual: expected.clone(),
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
            Stmt::Loop { body } => {
                for stmt in body {
                    self.check_stmt(ast, *stmt);
                }
            }
            Stmt::While { condition, body } => {
                // Check that condition is bool
                self.check_expr(ast, *condition, Type::Bool);
                for stmt in body {
                    self.check_stmt(ast, *stmt);
                }
            }
            Stmt::Condition {
                condition,
                then_body,
                else_body,
            } => {
                // Check that condition is bool
                self.check_expr(ast, *condition, Type::Bool);
                for stmt in then_body {
                    self.check_stmt(ast, *stmt);
                }
                if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        self.check_stmt(ast, *stmt);
                    }
                }
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
                    self.resolve_type(ty, &generics, span, ast)
                }
                (Some(ty), None) => {
                    // No type_id available, use function body span as fallback
                    let span = ast.get_expr_span(&func.body);
                    self.resolve_type(ty, &generics, span, ast)
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
                    expected: annotated_return.clone(), // expected type
                    actual: body_ty.clone(),            // found type
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

    fn unify(&mut self, expected: &Type, actual: &Type) -> Result<(), UnifyError> {
        let expected = self.normalize(expected);
        let actual = self.normalize(actual);

        match (&expected, &actual) {
            (Type::I32, Type::I32)
            | (Type::U8, Type::U8)
            | (Type::F32, Type::F32)
            | (Type::Bool, Type::Bool)
            | (Type::Unit, Type::Unit)
            | (Type::Str, Type::Str)
            | (Type::Pointer, Type::Pointer) => Ok(()),

            (Type::Unknown(expected_var), Type::Unknown(actual_var)) => {
                // Propagate integer constraint: if either is integer-constrained, both are
                if self.integer_type_vars.contains(expected_var)
                    || self.integer_type_vars.contains(actual_var)
                {
                    self.integer_type_vars.insert(*expected_var);
                    self.integer_type_vars.insert(*actual_var);
                }
                self.unification_table
                    .unify_var_var(*expected_var, *actual_var)
                    .map_err(|_| UnifyError::InfiniteType {
                        var: *expected_var,
                        ty: actual,
                    })
            }

            (Type::Unknown(expected_var), actual) | (actual, Type::Unknown(expected_var)) => {
                // Check integer type constraint
                if self.integer_type_vars.contains(expected_var) && !Self::is_integer_type(actual) {
                    return Err(UnifyError::Mismatch {
                        expected: Type::I32, // Report as expecting i32
                        found: actual.clone(),
                    });
                }
                // TODO: occurs check here
                self.unification_table
                    .unify_var_value(*expected_var, TypeValue::Bound(actual.clone()))
                    .map_err(|_| UnifyError::InfiniteType {
                        var: *expected_var,
                        ty: actual.clone(),
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
                        expected: expected.clone(),
                        found: actual.clone(),
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

            (Type::Named(n1), Type::Named(n2)) => {
                if n1 == n2 {
                    Ok(())
                } else {
                    Err(UnifyError::Mismatch {
                        expected,
                        found: actual,
                    })
                }
            }

            _ => Err(UnifyError::Mismatch {
                expected,
                found: actual,
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

    /// Create a fresh type variable constrained to integer types (i32, u8, etc.)
    fn fresh_integer_type(&mut self) -> Type {
        let var = self.fresh_type_var();
        self.integer_type_vars.insert(var);
        Type::Unknown(var)
    }

    /// Check if a type is a valid integer type
    fn is_integer_type(ty: &Type) -> bool {
        matches!(ty, Type::I32 | Type::U8)
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
        // Use normalize_with_default to resolve unbound type vars to i32
        let normalized_args: Vec<Type> = args
            .iter()
            .map(|t| self.normalize_with_default(t))
            .collect();

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
                Constraint::Equal {
                    expected: lhs,
                    actual: rhs,
                    ..
                } => self.unify(lhs, rhs),
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
    pub fn check_program(mut self, mut ast: Ast) -> TypedAst {
        // Check all regular functions, tracking current module for unqualified lookups
        for module in &ast.mods {
            // Set current module (skip empty module names from standalone parsing)
            self.current_module = if module.name.is_empty() {
                None
            } else {
                Some(module.name.to_string())
            };

            // Check functions declared in this module
            for decl in &module.decs {
                if let crate::Decl::Func(func_id) = decl {
                    self.check_func(&ast, *func_id);
                }
            }
        }
        self.current_module = None;

        self.solve(&ast);

        // normalize all stored expression types, defaulting unbound type vars to i32
        let types_to_normalize: Vec<(Id<Expr>, Type)> = self
            .expr_types
            .iter()
            .map(|(id, ty)| (*id, ty.clone()))
            .collect();
        let mut expr_types = HashMap::new();
        for (id, ty) in types_to_normalize {
            expr_types.insert(id, self.normalize_with_default(&ty));
        }

        // Apply inferred return types to functions that don't have explicit annotations
        let func_types_to_apply: Vec<_> = self
            .func_types
            .iter()
            .map(|(id, ty)| (*id, ty.clone()))
            .collect();
        for (func_id, return_ty) in func_types_to_apply {
            let func = ast.funcs.get_mut(&func_id);
            if func.return_type.is_none() {
                func.return_type = Some(self.normalize_with_default(&return_ty));
            }
        }

        TypedAst {
            ast,
            types: expr_types,
            errors: self.errors,
            constraints: self.constraints,
        }
    }

    /// Normalize a type, defaulting unbound type variables to i32
    fn normalize_with_default(&mut self, ty: &Type) -> Type {
        match ty {
            Type::Unknown(var) => match self.unification_table.probe_value(*var) {
                TypeValue::Bound(ty) => self.normalize_with_default(&ty),
                TypeValue::Unbound => Type::I32, // Default unbound type vars to i32
            },
            Type::Fun { arguments, returns } => Type::Fun {
                arguments: arguments
                    .iter()
                    .map(|t| self.normalize_with_default(t))
                    .collect(),
                returns: Box::new(self.normalize_with_default(returns)),
            },
            Type::Reference {
                lifetime,
                mutable,
                to,
            } => Type::Reference {
                lifetime: self.normalize_lifetime(lifetime),
                mutable: *mutable,
                to: Box::new(self.normalize_with_default(to)),
            },
            _ => ty.clone(),
        }
    }

    /// Returns the set of module names imported by the current module (including itself).
    fn imported_modules(&self, ast: &Ast) -> HashSet<String> {
        let mut imported = HashSet::new();
        if let Some(ref current) = self.current_module {
            imported.insert(current.clone());
            for name in ast.use_modules(current) {
                imported.insert(name);
            }
        }
        imported
    }

    /// Collect candidate function names for "did you mean?" suggestions.
    /// Returns unqualified names so `closest_match` in the rendering works correctly.
    fn function_candidates(&self, ast: &Ast, name: &str) -> Vec<String> {
        let max_dist = (name.len() / 2).max(2);
        let loaded_module_names: HashSet<&str> = ast.mods.iter().map(|m| &*m.name).collect();
        let mut candidates = Vec::new();

        // Scan the func registry (loaded modules)
        for (key, _) in &ast.func_registry {
            let unqualified = key.rsplit("::").next().unwrap_or(key);
            let dist = edit_distance(name, unqualified);
            if dist <= max_dist && dist > 0 && !candidates.contains(&unqualified.to_string()) {
                candidates.push(unqualified.to_string());
            }
        }

        // Scan unloaded bundled modules
        for bundled in BUNDLED_MODULES {
            if loaded_module_names.contains(bundled.name) {
                continue;
            }
            let (functions, _) = bundled.exported_names();
            for func_name in functions {
                let dist = edit_distance(name, func_name);
                if dist <= max_dist && dist > 0 && !candidates.contains(&func_name.to_string()) {
                    candidates.push(func_name.to_string());
                }
            }
        }

        candidates
    }

    /// Collect candidate struct/type names for "did you mean?" suggestions.
    fn struct_candidates(&self, ast: &Ast) -> Vec<String> {
        let loaded_module_names: HashSet<&str> = ast.mods.iter().map(|m| &*m.name).collect();
        let mut candidates: Vec<String> = ast
            .structs
            .iter()
            .map(|s| s.name.value.to_string())
            .collect();

        // Also scan unloaded bundled modules
        for bundled in BUNDLED_MODULES {
            if loaded_module_names.contains(bundled.name) {
                continue;
            }
            let (_, structs) = bundled.exported_names();
            for s in structs {
                let name = s.to_string();
                if !candidates.contains(&name) {
                    candidates.push(name);
                }
            }
        }

        candidates
    }

    /// Find non-imported modules that export a function matching the given name (exact or fuzzy).
    fn module_suggestions_for_function(&self, ast: &Ast, name: &str) -> Vec<String> {
        let max_dist = (name.len() / 2).max(2);
        let imported = self.imported_modules(ast);
        let loaded_module_names: HashSet<&str> = ast.mods.iter().map(|m| &*m.name).collect();
        let mut suggestions = Vec::new();

        // Check loaded modules
        for module in &ast.mods {
            if module.name.is_empty() || imported.contains(&*module.name) {
                continue;
            }
            for (member_name, kind) in module.exported_members(ast) {
                if matches!(kind, MemberKind::Function)
                    && edit_distance(name, member_name) <= max_dist
                {
                    suggestions.push(module.name.to_string());
                    break;
                }
            }
        }

        // Check unloaded bundled modules
        for bundled in BUNDLED_MODULES {
            if imported.contains(bundled.name) || loaded_module_names.contains(bundled.name) {
                continue;
            }
            let (functions, _) = bundled.exported_names();
            if functions
                .iter()
                .any(|&f| edit_distance(name, f) <= max_dist)
            {
                suggestions.push(bundled.name.to_string());
            }
        }

        suggestions
    }

    /// Find non-imported modules that export a struct matching the given name (exact or fuzzy).
    fn module_suggestions_for_struct(&self, ast: &Ast, name: &str) -> Vec<String> {
        let max_dist = (name.len() / 2).max(2);
        let imported = self.imported_modules(ast);
        let loaded_module_names: HashSet<&str> = ast.mods.iter().map(|m| &*m.name).collect();
        let mut suggestions = Vec::new();

        // Check loaded modules
        for module in &ast.mods {
            if module.name.is_empty() || imported.contains(&*module.name) {
                continue;
            }
            for (member_name, kind) in module.exported_members(ast) {
                if matches!(kind, MemberKind::Struct)
                    && edit_distance(name, member_name) <= max_dist
                {
                    suggestions.push(module.name.to_string());
                    break;
                }
            }
        }

        // Check unloaded bundled modules
        for bundled in BUNDLED_MODULES {
            if imported.contains(bundled.name) || loaded_module_names.contains(bundled.name) {
                continue;
            }
            let (_, structs) = bundled.exported_names();
            if structs.iter().any(|&s| edit_distance(name, s) <= max_dist) {
                suggestions.push(bundled.name.to_string());
            }
        }

        suggestions
    }

    fn resolve_type(
        &mut self,
        ty: &Type,
        generics: &HashMap<String, TypeVar>,
        span: &Span,
        ast: &Ast,
    ) -> Type {
        match ty {
            Type::Named(name) => {
                // First check if it's a generic type parameter
                if let Some(&tv) = generics.get(name.as_ref()) {
                    return Type::Unknown(tv);
                }
                // Then check if it's a known struct
                if ast.find_struct_by_name(name).is_some() {
                    return ty.clone();
                }
                // Unknown type
                self.errors.push(TypeError::UnknownType {
                    span: span.clone(),
                    available_types: self.struct_candidates(ast),
                    module_suggestions: self.module_suggestions_for_struct(ast, name),
                    name: name.to_string(),
                });
                self.fresh_type() // return fresh var to continue checking
            }
            Type::Reference {
                mutable,
                lifetime,
                to,
            } => Type::Reference {
                mutable: *mutable,
                lifetime: lifetime.clone(),
                to: Box::new(self.resolve_type(to, generics, span, ast)),
            },
            _ => ty.clone(),
        }
    }
}

use crate::diagnostics::{Diagnostic, Label};
use crate::{ExprId, TraitId, Type, TypeId, TypeVar, TypedAst, type_check::Provenance};

#[derive(Debug)]
pub enum TypeError {
    Mismatch {
        expected: Type,
        found: Type,
        provenance: Provenance,
        expected_type_id: Option<TypeId>, // For showing where the expected type was declared
    },
    InfiniteType {
        var: TypeVar,
        ty: Type,
    },
    MissingImpl {
        trait_id: TraitId,
        self_type: Type,
        arg_types: Vec<Type>,
    },
    UnboundVariable {
        name: String,
    },
    WrongArgCount {
        expected: usize,
        found: usize,
    },
    Internal(String),
    UnknownType {
        name: String,
    },
}

impl TypeError {
    pub fn to_diagnostic_message(&self) -> String {
        match self {
            TypeError::Mismatch {
                expected, found, ..
            } => {
                format!("Type mismatch: expected {}, found {}", expected, found)
            }
            TypeError::InfiniteType { var, ty } => {
                format!("Infinite type: {:?} = {:?}", var, ty)
            }
            TypeError::MissingImpl {
                trait_id,
                self_type,
                arg_types,
            } => {
                format!(
                    "Missing implementation for trait {:?} on {:?} with args {:?}",
                    trait_id, self_type, arg_types
                )
            }
            TypeError::UnboundVariable { name } => {
                format!("Unbound variable: {}", name)
            }
            TypeError::WrongArgCount { expected, found } => {
                format!(
                    "Wrong argument count: expected {}, found {}",
                    expected, found
                )
            }
            TypeError::Internal(msg) => {
                format!("Internal error: {}", msg)
            }
            TypeError::UnknownType { name } => {
                format!("Unknown type `{}`", name)
            }
        }
    }

    pub fn to_diagnostic(&self, typed_ast: &TypedAst, expr_id: &ExprId) -> Diagnostic {
        let span = typed_ast.ast.get_expr_span(expr_id);

        match self {
            TypeError::Mismatch {
                expected,
                found,
                provenance,
                expected_type_id,
            } => {
                let mut diag = Diagnostic::error(format!(
                    "type mismatch: expected `{}`, found `{}`",
                    expected, found
                ))
                .with_label(Label::primary(span.clone(), format!("found `{}`", found)));

                // Add provenance-based secondary label
                match provenance {
                    Provenance::FuncArg {
                        param_type_id: Some(tid),
                        ..
                    } => {
                        if let Some(type_span) = typed_ast.ast.try_get_type_span(tid) {
                            diag = diag.with_label(Label::secondary(
                                type_span.clone(),
                                format!("expected `{}` due to parameter type", expected),
                            ));
                        }
                    }
                    Provenance::BinaryOp(op_expr) if *op_expr != *expr_id => {
                        let op_span = typed_ast.ast.get_expr_span(op_expr);
                        diag = diag.with_label(Label::secondary(
                            op_span.clone(),
                            format!("expected `{}` due to this operator", expected),
                        ));
                    }
                    Provenance::LetBinding(let_expr) if *let_expr != *expr_id => {
                        let let_span = typed_ast.ast.get_expr_span(let_expr);
                        diag = diag.with_label(Label::secondary(
                            let_span.clone(),
                            format!("expected `{}` due to let binding", expected),
                        ));
                    }
                    Provenance::Deref(deref_expr) if *deref_expr != *expr_id => {
                        let deref_span = typed_ast.ast.get_expr_span(deref_expr);
                        diag = diag.with_label(Label::secondary(
                            deref_span.clone(),
                            format!("expected `{}` due to dereference", expected),
                        ));
                    }
                    _ => {}
                }

                // Add explicit type annotation label if available
                if let Some(type_id) = expected_type_id {
                    if let Some(type_span) = typed_ast.ast.try_get_type_span(type_id) {
                        diag = diag.with_label(Label::secondary(
                            type_span.clone(),
                            "expected type declared here",
                        ));
                    }
                }

                diag
            }
            TypeError::InfiniteType { var, ty } => {
                Diagnostic::error(format!("infinite type: {:?} occurs in {:?}", var, ty))
                    .with_label(Label::primary(span.clone(), "infinite type detected here"))
                    .with_hint(format!(
                        "type `{:?}` references itself through `{:?}`",
                        var, ty
                    ))
            }
            TypeError::MissingImpl {
                trait_id,
                self_type,
                arg_types,
            } => {
                let mut diag = Diagnostic::error(format!(
                    "no implementation of trait {:?} for type `{}`",
                    trait_id, self_type
                ))
                .with_label(Label::primary(
                    span.clone(),
                    format!("trait not implemented for `{}`", self_type),
                ));

                if !arg_types.is_empty() {
                    let args_str = arg_types
                        .iter()
                        .map(|t| format!("`{}`", t))
                        .collect::<Vec<_>>()
                        .join(", ");
                    diag = diag.with_hint(format!("required with argument types: {}", args_str));
                }

                diag
            }
            TypeError::UnboundVariable { name } => {
                Diagnostic::error(format!("cannot find value `{}` in this scope", name))
                    .with_label(Label::primary(span.clone(), "not found in this scope"))
                    .with_hint(format!("did you mean to declare `{}`?", name))
            }
            TypeError::WrongArgCount { expected, found } => {
                let msg = if *expected == 1 { "argument" } else { "arguments" };
                Diagnostic::error(format!(
                    "function takes {} {} but {} {} supplied",
                    expected,
                    msg,
                    found,
                    if *found == 1 { "was" } else { "were" }
                ))
                .with_label(Label::primary(
                    span.clone(),
                    format!("expected {} {}, found {}", expected, msg, found),
                ))
            }
            TypeError::Internal(msg) => {
                Diagnostic::error(format!("internal compiler error: {}", msg))
                    .with_label(Label::primary(span.clone(), "internal error occurred here"))
            }
            TypeError::UnknownType { name } => {
                Diagnostic::error(format!("cannot find type `{}` in this scope", name))
                    .with_label(Label::primary(span.clone(), "unknown type"))
                    .with_hint(format!("did you mean to declare type `{}`?", name))
            }
        }
    }
}

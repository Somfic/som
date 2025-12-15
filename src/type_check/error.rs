use crate::ast::{
    TRAIT_ADD, TRAIT_DIV, TRAIT_EQ, TRAIT_GT, TRAIT_GT_EQ, TRAIT_LT, TRAIT_LT_EQ, TRAIT_MUL,
    TRAIT_NEQ, TRAIT_SUB,
};
use crate::diagnostics::{Diagnostic, Label};
use crate::span::Span;
use crate::{Ast, TraitId, Type, TypeVar, type_check::Provenance};

#[derive(Debug)]
pub enum TypeError {
    Mismatch {
        span: Span,
        expected: Type,
        found: Type,
        provenance: Provenance,
    },
    InfiniteType {
        span: Span,
        var: TypeVar,
        ty: Type,
    },
    MissingImpl {
        span: Span,
        trait_id: TraitId,
        self_type: Type,
        arg_types: Vec<Type>,
    },
    UnboundVariable {
        span: Span,
        name: String,
    },
    WrongArgCount {
        span: Span,
        expected: usize,
        found: usize,
    },
    Internal {
        span: Span,
        message: String,
    },
    UnknownType {
        span: Span,
        name: String,
    },
}

impl TypeError {
    pub fn to_diagnostic(&self, ast: &Ast) -> Diagnostic {
        match self {
            TypeError::Mismatch {
                span,
                expected,
                found,
                provenance,
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
                        if let Some(type_span) = ast.try_get_type_span(tid) {
                            diag = diag.with_label(Label::secondary(
                                type_span.clone(),
                                format!("expected `{}` due to parameter type", expected),
                            ));
                        }
                    }
                    Provenance::BinaryOp(op_expr) => {
                        let op_span = ast.get_expr_span(op_expr);
                        if op_span != span {
                            diag = diag.with_label(Label::secondary(
                                op_span.clone(),
                                format!("expected `{}` due to this operator", expected),
                            ));
                        }
                    }
                    Provenance::LetBinding(let_expr) => {
                        let let_span = ast.get_expr_span(let_expr);
                        if let_span != span {
                            diag = diag.with_label(Label::secondary(
                                let_span.clone(),
                                format!("expected `{}` due to let binding", expected),
                            ));
                        }
                    }
                    Provenance::Deref(deref_expr) => {
                        let deref_span = ast.get_expr_span(deref_expr);
                        if deref_span != span {
                            diag = diag.with_label(Label::secondary(
                                deref_span.clone(),
                                format!("expected `{}` due to dereference", expected),
                            ));
                        }
                    }
                    _ => {}
                }

                diag
            }
            TypeError::InfiniteType { span, var, ty } => {
                Diagnostic::error(format!("infinite type: {:?} occurs in {:?}", var, ty))
                    .with_label(Label::primary(span.clone(), "infinite type detected here"))
                    .with_hint(format!(
                        "type `{:?}` references itself through `{:?}`",
                        var, ty
                    ))
            }
            TypeError::MissingImpl {
                span,
                trait_id,
                self_type,
                arg_types,
            } => {
                // Generate natural error messages for binary operators
                let rhs = arg_types.first();
                let (msg, label) = match (*trait_id, rhs) {
                    (TRAIT_ADD, Some(rhs)) => (
                        format!("cannot add `{}` and `{}`", self_type, rhs),
                        format!("no implementation for `{} + {}`", self_type, rhs),
                    ),
                    (TRAIT_SUB, Some(rhs)) => (
                        format!("cannot subtract `{}` from `{}`", rhs, self_type),
                        format!("no implementation for `{} - {}`", self_type, rhs),
                    ),
                    (TRAIT_MUL, Some(rhs)) => (
                        format!("cannot multiply `{}` and `{}`", self_type, rhs),
                        format!("no implementation for `{} * {}`", self_type, rhs),
                    ),
                    (TRAIT_DIV, Some(rhs)) => (
                        format!("cannot divide `{}` by `{}`", self_type, rhs),
                        format!("no implementation for `{} / {}`", self_type, rhs),
                    ),
                    (TRAIT_EQ, Some(rhs)) => (
                        format!("cannot compare `{}` and `{}` for equality", self_type, rhs),
                        format!("no implementation for `{} == {}`", self_type, rhs),
                    ),
                    (TRAIT_NEQ, Some(rhs)) => (
                        format!(
                            "cannot compare `{}` and `{}` for inequality",
                            self_type, rhs
                        ),
                        format!("no implementation for `{} != {}`", self_type, rhs),
                    ),
                    (TRAIT_LT, Some(rhs)) => (
                        format!("cannot compare `{}` < `{}`", self_type, rhs),
                        format!("no implementation for `{} < {}`", self_type, rhs),
                    ),
                    (TRAIT_GT, Some(rhs)) => (
                        format!("cannot compare `{}` > `{}`", self_type, rhs),
                        format!("no implementation for `{} > {}`", self_type, rhs),
                    ),
                    (TRAIT_LT_EQ, Some(rhs)) => (
                        format!("cannot compare `{}` <= `{}`", self_type, rhs),
                        format!("no implementation for `{} <= {}`", self_type, rhs),
                    ),
                    (TRAIT_GT_EQ, Some(rhs)) => (
                        format!("cannot compare `{}` >= `{}`", self_type, rhs),
                        format!("no implementation for `{} >= {}`", self_type, rhs),
                    ),
                    _ => (
                        format!(
                            "no implementation of `{}` for type `{}`",
                            trait_id, self_type
                        ),
                        format!("`{}` not implemented for `{}`", trait_id, self_type),
                    ),
                };

                let diag = Diagnostic::error(msg).with_label(Label::primary(span.clone(), label));

                // Add help note for implementing the trait
                let help = match (*trait_id, rhs) {
                    (TRAIT_ADD, Some(rhs)) => Some(format!(
                        "consider implementing `Add<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_SUB, Some(rhs)) => Some(format!(
                        "consider implementing `Sub<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_MUL, Some(rhs)) => Some(format!(
                        "consider implementing `Mul<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_DIV, Some(rhs)) => Some(format!(
                        "consider implementing `Div<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_EQ, Some(rhs)) => Some(format!(
                        "consider implementing `Eq<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_NEQ, Some(rhs)) => Some(format!(
                        "consider implementing `NotEq<{}>` for `{}`",
                        rhs, self_type
                    )),
                    (TRAIT_LT | TRAIT_GT | TRAIT_LT_EQ | TRAIT_GT_EQ, Some(rhs)) => Some(format!(
                        "consider implementing `Ord<{}>` for `{}`",
                        rhs, self_type
                    )),
                    _ => None,
                };

                match help {
                    Some(h) => diag.with_hint(h),
                    None => diag,
                }
            }
            TypeError::UnboundVariable { span, name } => {
                Diagnostic::error(format!("cannot find value `{}` in this scope", name))
                    .with_label(Label::primary(span.clone(), "not found in this scope"))
                    .with_hint(format!("did you mean to declare `{}`?", name))
            }
            TypeError::WrongArgCount {
                span,
                expected,
                found,
            } => {
                let msg = if *expected == 1 {
                    "argument"
                } else {
                    "arguments"
                };
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
            TypeError::Internal { span, message } => {
                Diagnostic::error(format!("internal compiler error: {}", message))
                    .with_label(Label::primary(span.clone(), "internal error occurred here"))
            }
            TypeError::UnknownType { span, name } => {
                Diagnostic::error(format!("cannot find type `{}` in this scope", name))
                    .with_label(Label::primary(span.clone(), "unknown type"))
                    .with_hint(format!("did you mean to declare type `{}`?", name))
            }
        }
    }
}

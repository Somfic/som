use crate::Trait;
use crate::arena::Id;
use crate::ast::{
    TRAIT_ADD, TRAIT_DIV, TRAIT_EQ, TRAIT_GT, TRAIT_GT_EQ, TRAIT_LT, TRAIT_LT_EQ, TRAIT_MUL,
    TRAIT_NEQ, TRAIT_SUB,
};
use crate::diagnostics::{Diagnostic, Highlight, Label, closest_match};
use crate::span::Span;
use crate::{Ast, Type, TypeVar, type_check::Provenance};

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
        trait_id: Id<Trait>,
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
        available_types: Vec<String>,
        module_suggestions: Vec<String>,
    },
    UnknownFunction {
        span: Span,
        name: String,
        available_functions: Vec<String>,
        module_suggestions: Vec<String>,
    },
    UnknownStruct {
        span: Span,
        name: String,
        available_structs: Vec<String>,
        module_suggestions: Vec<String>,
    },
    MissingField {
        span: Span,
        struct_name: String,
        field_name: String,
    },
    UnknownField {
        span: Span,
        struct_name: String,
        field_name: String,
        available_fields: Vec<String>,
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
                    "type mismatch: expected {} but found {}",
                    expected.as_type(),
                    found.as_type()
                ))
                .with_label(Label::primary(
                    span.clone(),
                    format!("expected {}", expected.as_type()),
                ))
                .with_hint(format!(
                    "found {} where {} was expected",
                    found.plain_language().as_type(),
                    expected.plain_language().as_type()
                ));

                // Add provenance-based secondary label
                match provenance {
                    Provenance::FuncArg {
                        param_type_id: Some(tid),
                        ..
                    } => {
                        if let Some(type_span) = ast.try_get_type_span(tid) {
                            diag = diag.with_label(Label::secondary(
                                type_span.clone(),
                                format!("{} defined here", expected.as_type()),
                            ));
                        }
                    }
                    Provenance::FunctionCall(_, Some(tid)) => {
                        if let Some(type_span) = ast.try_get_type_span(tid) {
                            diag = diag.with_label(Label::secondary(
                                type_span.clone(),
                                format!("{} defined here", expected.as_type()),
                            ));
                        }
                    }
                    Provenance::BinaryOp(op_expr) => {
                        let op_span = ast.get_expr_span(op_expr);
                        if op_span != span {
                            diag =
                                diag.with_label(Label::secondary(op_span.clone(), "operator here"));
                        }
                    }
                    Provenance::LetBinding(let_expr) => {
                        let let_span = ast.get_expr_span(let_expr);
                        if let_span != span {
                            diag =
                                diag.with_label(Label::secondary(let_span.clone(), "binding here"));
                        }
                    }
                    Provenance::Annotation(ann_expr) => {
                        let ann_span = ast.get_expr_span(ann_expr);
                        if ann_span != span {
                            diag = diag
                                .with_label(Label::secondary(ann_span.clone(), "annotation here"));
                        }
                    }
                    Provenance::Deref(deref_expr) => {
                        let deref_span = ast.get_expr_span(deref_expr);
                        if deref_span != span {
                            diag = diag.with_label(Label::secondary(
                                deref_span.clone(),
                                "dereference here",
                            ));
                        }
                    }
                    Provenance::Conditional(cond_expr) => {
                        let cond_span = ast.get_expr_span(cond_expr);
                        if cond_span != span {
                            diag = diag.with_label(Label::secondary(
                                cond_span.clone(),
                                "branches diverge here",
                            ));
                        }
                    }
                    Provenance::ConstructorField(field_expr) => {
                        let field_span = ast.get_expr_span(field_expr);
                        if field_span != span {
                            diag = diag.with_label(Label::secondary(
                                field_span.clone(),
                                "field defined here",
                            ));
                        }
                    }
                    Provenance::Assignment(target_expr) => {
                        let target_span = ast.get_expr_span(target_expr);
                        if target_span != span {
                            diag = diag.with_label(Label::secondary(
                                target_span.clone(),
                                "target defined here",
                            ));
                        }
                    }
                    Provenance::Not(not_expr) => {
                        let not_span = ast.get_expr_span(not_expr);
                        if not_span != span {
                            diag = diag
                                .with_label(Label::secondary(not_span.clone(), "negation here"));
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
            } => Diagnostic::error(format!(
                "no implementation of {} for type {}",
                trait_id.as_type(),
                self_type.as_type()
            ))
            .with_label(Label::primary(span.clone(), "trait not implemented")),
            TypeError::UnboundVariable { span, name } => {
                Diagnostic::error(format!("cannot find value {} in this scope", name.as_var()))
                    .with_label(Label::primary(span.clone(), "not found in this scope"))
                    .with_hint(format!("did you mean to declare {}?", name.as_var()))
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
            TypeError::UnknownType {
                span,
                name,
                available_types,
                module_suggestions,
            } => {
                let max_dist = (name.len() / 2).max(2);
                let mut diag = Diagnostic::error(format!("cannot find type {}", name.as_type()))
                    .with_label(Label::primary(span.clone(), "unknown type"));

                if let Some(suggestion) = closest_match(name, available_types, max_dist) {
                    diag = diag.with_hint(format!("did you mean {}?", suggestion.as_type()));
                }
                for module in module_suggestions {
                    diag = diag.with_hint(format!(
                        "a type with this name exists in module {}. try `use {};`",
                        module.as_module(),
                        module
                    ));
                }
                diag
            }
            TypeError::UnknownFunction {
                span,
                name,
                available_functions,
                module_suggestions,
            } => {
                let max_dist = (name.len() / 2).max(2);
                let mut diag =
                    Diagnostic::error(format!("cannot find function {}", name.as_func()))
                        .with_label(Label::primary(span.clone(), "unknown function"));

                if let Some(suggestion) = closest_match(name, available_functions, max_dist) {
                    diag = diag.with_hint(format!("did you mean {}?", suggestion.as_func()));
                }
                for module in module_suggestions {
                    diag = diag.with_hint(format!(
                        "a function with this name exists in module {}. try `use {};`",
                        module.as_module(),
                        module
                    ));
                }
                diag
            }
            TypeError::UnknownStruct {
                span,
                name,
                available_structs,
                module_suggestions,
            } => {
                let max_dist = (name.len() / 2).max(2);
                let mut diag =
                    Diagnostic::error(format!("cannot find struct {}", name.as_struct()))
                        .with_label(Label::primary(span.clone(), "unknown struct"));

                if let Some(suggestion) = closest_match(name, available_structs, max_dist) {
                    diag = diag.with_hint(format!("did you mean {}?", suggestion.as_struct()));
                }
                for module in module_suggestions {
                    diag = diag.with_hint(format!(
                        "a struct with this name exists in module {}. try `use {};`",
                        module.as_module(),
                        module
                    ));
                }
                diag
            }
            TypeError::MissingField {
                span,
                struct_name,
                field_name,
            } => Diagnostic::error(format!(
                "missing field {} in initializer of {}",
                field_name.as_field(),
                struct_name.as_struct()
            ))
            .with_label(Label::primary(
                span.clone(),
                format!("missing {}", field_name.as_field()),
            )),
            TypeError::UnknownField {
                span,
                struct_name,
                field_name,
                available_fields,
            } => {
                let max_dist = (field_name.len() / 2).max(2);
                let diag = Diagnostic::error(format!(
                    "struct {} has no field named {}",
                    struct_name.as_struct(),
                    field_name.as_field()
                ))
                .with_label(Label::primary(span.clone(), "unknown field"));

                match closest_match(field_name, available_fields, max_dist) {
                    Some(suggestion) => {
                        diag.with_hint(format!("did you mean {}?", suggestion.as_field()))
                    }
                    None => diag,
                }
            }
        }
    }
}

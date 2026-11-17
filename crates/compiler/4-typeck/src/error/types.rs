use som_base::message::{code, struct_name};
use som_base::{Diagnostic, Label, msg};

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::WrongTypeArgCount {
            name,
            expected,
            found,
            span,
        } => {
            let plural = |n: &usize| if *n == 1 { "" } else { "s" };
            Diagnostic::error(msg![
                struct_name(name),
                format!(
                    " takes {expected} type argument{}, but {found} ",
                    plural(expected)
                ),
                if *found == 1 {
                    "was given"
                } else {
                    "were given"
                }
            ])
            .with_label(Label::primary(span.clone(), "here"))
        }
        TypeError::WrongTypeArgRange {
            name,
            required,
            most,
            found,
            span,
        } => Diagnostic::error(msg![
            struct_name(name),
            format!(" takes {required} to {most} type arguments, but {found} "),
            if *found == 1 {
                "was given"
            } else {
                "were given"
            }
        ])
        .with_label(Label::primary(span.clone(), "here"))
        .with_hint(msg![match most - required {
            1 => "the last one has a default, and can be left out".to_string(),
            n => format!("the last {n} have defaults, and can be left out"),
        }]),
        TypeError::AliasCycle { name, span } => {
            Diagnostic::error(msg!["type alias ", struct_name(name), " refers to itself"])
                .with_label(Label::primary(span.clone(), "used in its own definition"))
                .with_hint(msg![
                    "an alias stands for its type everywhere, so it can't contain itself"
                ])
        }
        TypeError::ValueAsType { what, span } => {
            Diagnostic::error(msg![what.clone(), " is a value, not a type"])
                .with_label(Label::primary(span.clone(), "a type is needed here"))
        }
        TypeError::TypeAsValue { what, ty, span } => {
            let wanted = match ty {
                Some(ty) => msg!["a value of type ", ty.clone(), " is needed here"],
                None => msg!["a number is needed here"],
            };
            Diagnostic::error(msg![what.clone(), " is a type, not a value"])
                .with_label(Label::primary(span.clone(), wanted))
        }
        TypeError::InvalidValueParamType { ty, span } => {
            Diagnostic::error(msg!["a value parameter can't be of type ", ty.clone()]).with_label(
                Label::primary(span.clone(), msg!["not an integer or ", code("bool")]),
            )
        }
        _ => unreachable!("not one of types as written"),
    }
}

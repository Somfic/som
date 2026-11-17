use som_base::message::{code, var};
use som_base::{Diagnostic, Label, msg};

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::ConstNotCount {
            name,
            span,
            defined_at,
        } => Diagnostic::error(msg!["constant ", var(name), " isn't a count"])
            .with_label(Label::primary(
                span.clone(),
                "a whole number, at least 0, is needed here",
            ))
            .with_label(Label::secondary(defined_at.clone(), "defined here")),
        TypeError::NotSettled { what, hint, span } => {
            Diagnostic::error(msg![*what, " has to be known while compiling"])
                .with_label(Label::primary(
                    span.clone(),
                    "only known when the program runs",
                ))
                .with_hint(*hint)
        }
        TypeError::ReflectNotStruct { name, span } => Diagnostic::error(msg![
            code(name),
            " isn't a struct to go through the fields of"
        ])
        .with_label(Label::primary(span.clone(), "not a struct here"))
        .with_hint(msg![
            code("@for f in T"),
            " goes through a struct's fields: a struct, or a type parameter used \
                     with one"
        ]),
        TypeError::ReflectNotEnum { name, span } => Diagnostic::error(msg![
            code(name),
            " isn't an enum to go through the variants of"
        ])
        .with_label(Label::primary(span.clone(), "not an enum here"))
        .with_hint(msg![
            code("@for (i, k) in T"),
            " among a match's arms goes through an enum's variants: an enum, or a \
                     type parameter used with one"
        ]),
        TypeError::TypeNotCompared { span } => {
            Diagnostic::error("a type can only be compared with another type")
                .with_label(Label::primary(span.clone(), "a type, where a value goes"))
                .with_hint(msg![
                    "an ",
                    code("@if"),
                    " can ask which type it is: ",
                    code("@if T == f32 { … }")
                ])
        }
        TypeError::ConstCycle { name, span } => Diagnostic::error(msg![
            "constant ",
            var(name),
            " is needed to work itself out"
        ])
        .with_label(Label::primary(
            span.clone(),
            "what it runs needs its value, in a type or an `@if`",
        ))
        .with_hint("its value can't be known before it's worked out"),
        TypeError::MemberNotHere {
            name,
            ty,
            condition,
            span,
        } => {
            Diagnostic::error(msg![ty.clone(), " has no ", code(name)]).with_label(Label::primary(
                span.clone(),
                msg![code(name), " is only there where ", code(condition)],
            ))
        }
        TypeError::MemberMaybeNotHere {
            name,
            condition,
            span,
        } => Diagnostic::error(msg!["can't tell ", code(name), " is there"])
            .with_label(Label::primary(
                span.clone(),
                msg![
                    "only there where ",
                    code(condition),
                    ", which isn't known here"
                ],
            ))
            .with_hint(msg![
                "use it in ",
                code(format!("@if {condition} {{ … }}")),
                ", which is checked for each instance"
            ]),
        _ => unreachable!("not one of what's worked out while compiling"),
    }
}

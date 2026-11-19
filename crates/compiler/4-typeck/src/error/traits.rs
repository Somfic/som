use som_base::message::code;
use som_base::{Diagnostic, Fix, Label, msg};
use som_lang::LangTrait;

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::MissingTraitItem {
            missing,
            trait_,
            span,
            fix,
        } => {
            let first = missing.first().cloned().unwrap_or_default();
            let more = match missing.len() {
                0 | 1 => String::new(),
                n => format!(" and {} more", n - 1),
            };
            let items = if missing.len() == 1 { "it" } else { "them" };
            let mut d = Diagnostic::error(msg![
                "this implementation of ",
                code(trait_),
                " is missing ",
                code(first),
                more
            ])
            .with_label(Label::primary(span.clone(), "not everything is written"))
            .with_fix(Fix {
                title: msg![format!("write {items}")],
                edits: vec![fix.clone()],
            });
            if missing.len() > 1 {
                let rest: Vec<String> = missing.iter().skip(1).map(|m| format!("`{m}`")).collect();
                d = d.with_hint(format!("also missing {}", rest.join(", ")));
            }
            d
        }
        TypeError::NotInTrait {
            what,
            name,
            trait_,
            span,
        } => Diagnostic::error(msg![
            code(name),
            format!(" isn't a {what} of "),
            code(trait_)
        ])
        .with_label(Label::primary(span.clone(), "not in the trait"))
        .with_hint(msg![
            "put it in an ",
            code("impl"),
            " of its own, without the trait"
        ]),
        TypeError::ImplSignatureMismatch {
            name,
            expected,
            span,
            declared_at,
        } => Diagnostic::error(msg![code(name), " doesn't match the trait"])
            .with_label(Label::primary(
                span.clone(),
                msg!["the trait wants ", code(expected)],
            ))
            .with_label(Label::secondary(declared_at.clone(), "declared here")),
        TypeError::DoesNotImplement { ty, trait_, span } => {
            Diagnostic::error(msg![ty.clone(), " doesn't implement ", code(trait_)]).with_label(
                Label::primary(span.clone(), msg![code(trait_), " is needed here"]),
            )
        }
        TypeError::MissingSuperImpl {
            ty,
            trait_,
            missing,
            span,
        } => Diagnostic::error(msg![
            ty.clone(),
            " implements ",
            code(trait_),
            " but not ",
            code(missing),
            ", which it builds on"
        ])
        .with_label(Label::primary(
            span.clone(),
            msg![code(trait_), " needs ", code(missing), " too"],
        ))
        .with_hint(msg![
            "add ",
            code(format!("impl {missing} for …")),
            " for the same type"
        ]),
        TypeError::NotCalledSo { ty, signature, span } => Diagnostic::error(msg![
            ty.clone(),
            " can't be called as ",
            code(signature)
        ])
        .with_label(Label::primary(
            span.clone(),
            msg!["something called as ", code(signature), " is needed here"],
        )),
        TypeError::CopyWithMovedField {
            ty,
            field,
            field_ty,
            span,
        } => Diagnostic::error(msg![
            ty.clone(),
            " can't implement ",
            code(LangTrait::Copy.name())
        ])
        .with_label(Label::primary(
            span.clone(),
            msg![
                code(field.clone()),
                " is a ",
                field_ty.clone(),
                ", which is moved, not copied"
            ],
        )),
        TypeError::NotDisplay { ty, span } => {
            Diagnostic::error(msg![ty.clone(), " can't be written into text"])
                .with_label(Label::primary(
                    span.clone(),
                    msg!["it doesn't implement ", code(LangTrait::Display.name())],
                ))
                .with_hint(msg![
                    "implement ",
                    code(LangTrait::Display.name()),
                    " for it: ",
                    code("prop display_len"),
                    " says how many bytes its text takes, and ",
                    code("display"),
                    " writes them"
                ])
        }
        TypeError::DropOnCopy { ty, span } => Diagnostic::error(msg![
            ty.clone(),
            " can't implement ",
            code(LangTrait::Drop.name())
        ])
        .with_label(Label::primary(
            span.clone(),
            "its values are copied, so each copy would be dropped",
        )),
        TypeError::ShareOnCopy { ty, span } => Diagnostic::error(msg![
            ty.clone(),
            " can't implement ",
            code(LangTrait::Share.name())
        ])
        .with_label(Label::primary(
            span.clone(),
            msg![
                "it's ",
                code(LangTrait::Copy.name()),
                ": its copies are made bit for bit, so ",
                code("share"),
                " would never be called"
            ],
        )),
        TypeError::BuiltinImpl { ty, trait_, span } => {
            Diagnostic::error(msg![ty.clone(), " can't implement ", code(trait_)])
                .with_label(Label::primary(span.clone(), "its operators are built in"))
        }
        _ => unreachable!("not one of traits and their implementations"),
    }
}

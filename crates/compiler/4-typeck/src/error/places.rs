use som_base::message::{code, field, struct_name, var};
use som_base::{Diagnostic, Fix, Label, msg};
use som_lang::LangTrait;

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::CaptureChanged { name, span, moved } => {
            let (what, label) = match moved {
                true => ("moved", "moved here"),
                false => ("changed", "changed here"),
            };
            Diagnostic::error(msg![var(name), format!(" can't be {what} inside a closure")])
                .with_label(Label::primary(span.clone(), label))
                .with_hint("a closure only reads the variables around it")
        }
        TypeError::NotMutable {
            name,
            span,
            declared_at,
            borrow,
        } => {
            let label = if *borrow {
                "borrowed as mutable here"
            } else {
                "assigned here"
            };
            let d = Diagnostic::error(msg![var(name), " is not mutable"])
                .with_label(Label::primary(span.clone(), label))
                .with_label(Label::secondary(
                    declared_at.clone(),
                    msg!["declared without ", code("mut")],
                ));
            // A method's `self`, taken by value: what it changes is its own copy, gone
            // when it returns. Changing what it's called on takes `&mut self`.
            let source = declared_at.source.content();
            let end = (declared_at.start_offset + declared_at.length).min(source.len());
            let typed = source[end..].trim_start().starts_with(':');
            if name == som_lang::SELF_VALUE && !typed {
                d.with_hint(msg![
                    code("mut self"),
                    " would change the method's own copy, gone when it returns"
                ])
                .with_fix(Fix::replace(
                    msg!["take it by mutable reference: ", code("&mut self")],
                    declared_at.clone(),
                    "&mut self",
                ))
            } else {
                d.with_fix(Fix::replace(
                    msg!["make ", var(name), " mutable"],
                    declared_at.clone(),
                    format!("mut {name}"),
                ))
            }
        }
        TypeError::BehindSharedReference { span, borrow } => {
            let message = if *borrow {
                "cannot borrow as mutable through a `&` reference"
            } else {
                "cannot assign through a `&` reference"
            };
            Diagnostic::error(message)
                .with_label(Label::primary(
                    span.clone(),
                    "this is behind a `&` reference",
                ))
                .with_hint(msg!["changing it needs a ", code("&mut"), " reference"])
        }
        TypeError::AssignToProp {
            name,
            span,
            field: hidden,
        } => match hidden {
            Some((owner, hidden)) => Diagnostic::error(msg![
                "cannot assign to ",
                field(name.clone()),
                ": it's read-only here"
            ])
            .with_label(Label::primary(
                span.clone(),
                msg!["this is the property ", field(name.clone())],
            ))
            .with_label(Label::secondary(
                hidden.defined_at.clone(),
                msg![
                    "the field ",
                    field(name.clone()),
                    " can't be seen from here"
                ],
            ))
            .with_hint(msg![
                "change it through one of ",
                struct_name(owner.clone()),
                "'s methods"
            ]),
            None => Diagnostic::error(msg!["cannot assign to property ", field(name.clone())])
                .with_label(Label::primary(span.clone(), "this is a property"))
                .with_hint("a property is worked out when it's read; there's nothing to assign to"),
        },
        TypeError::InvalidAssignTarget { span } => {
            Diagnostic::error("cannot assign to this expression").with_label(Label::primary(
                span.clone(),
                "expected a variable, field or dereference",
            ))
        }
        TypeError::SharedChanged { span, borrow } => {
            let what = if *borrow {
                "cannot borrow what a `#T` shares as mutable"
            } else {
                "cannot change what a `#T` shares"
            };
            Diagnostic::error(what)
                .with_label(Label::primary(span.clone(), "every copy of it sees this"))
                .with_hint(msg![
                    "make a changed copy and share that: ",
                    code("Rc::new(…)")
                ])
        }
        TypeError::ReadOnlyStr { span } => {
            Diagnostic::error(msg!["cannot change the bytes of a ", code("str")])
                .with_label(Label::primary(span.clone(), "text can only be read"))
        }
        TypeError::ReadOnlySlice { span } => {
            Diagnostic::error(msg!["cannot change the elements of a ", code("[T]")])
                .with_label(Label::primary(span.clone(), "this slice can only read"))
                .with_hint(msg!["changing them needs a ", code("&mut [T]")])
        }
        TypeError::SliceIndexWrite { ty, span, instead } => {
            Diagnostic::error(msg![ty.clone(), " can't be set by index"])
                .with_label(Label::primary(
                    span.clone(),
                    "what's there is only known as the program runs: this might miss",
                ))
                .with_hint(msg![
                    "use ",
                    code(instead.clone()),
                    ", which says whether it's there"
                ])
        }
        TypeError::MoveOutOfReference {
            place,
            ty,
            span,
            fix,
        } => {
            let d = Diagnostic::error(msg![
                "cannot move ",
                code(place.clone()),
                " out of a reference"
            ])
            .with_label(Label::primary(
                span.clone(),
                msg![ty.clone(), " may be a type that moves, not copies"],
            ))
            .with_hint(msg![
                "borrow it instead, or require that it copies with ",
                code(LangTrait::Copy.name())
            ]);
            match fix {
                Some(fix) => d.with_fix(fix.clone()),
                None => d,
            }
        }
        TypeError::ConstChanged {
            name,
            span,
            defined_at,
            borrow,
        } => {
            let label = match borrow {
                true => "borrowed as mutable here",
                false => "changed here",
            };
            Diagnostic::error(msg!["constant ", var(name), " can't be changed"])
                .with_label(Label::primary(span.clone(), label))
                .with_label(Label::secondary(defined_at.clone(), "defined here"))
                .with_hint(msg![
                    "for a value that changes, copy it into a variable: ",
                    code(format!("let mut value = {name};"))
                ])
        }
        TypeError::ReceiverBehindReference { ty, span } => Diagnostic::error(msg![
            "the method takes the ",
            ty.clone(),
            " itself, but this is a reference to one"
        ])
        .with_label(Label::primary(
            span.clone(),
            "it can't be moved out of the reference",
        ))
        .with_hint(msg!["have the method take ", code("self: &…"), " instead"]),
        _ => unreachable!(
            "not one of changing what can't be changed, and moving what can't be moved"
        ),
    }
}

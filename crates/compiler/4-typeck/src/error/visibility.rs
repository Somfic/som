use som_base::message::{Message, field, struct_name};
use som_base::{Diagnostic, Label, msg};

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::NotVisible { name, span, hidden } => {
            som_resolve::visibility::diagnostic(name, span, hidden)
        }
        TypeError::ConstructHidden {
            ty,
            fields,
            span,
            fixes,
        } => {
            let mut hint = msg![struct_name(ty.clone()), " has private fields: "];
            hint.push(Message::list(
                fields.iter().map(|(name, _)| field(name)),
                " and ",
            ));
            let mut d =
                Diagnostic::error(msg!["cannot construct ", struct_name(ty.clone()), " here"])
                    .with_label(Label::primary(
                        span.clone(),
                        "not all of its fields can be set here",
                    ));
            for (name, at) in fields {
                d = d.with_label(Label::secondary(
                    at.clone(),
                    msg![field(name.clone()), " can't be seen from here"],
                ));
            }
            d = d.with_hint(hint);
            if fixes.is_empty() {
                d = d.with_hint("only its own functions can make one");
            }
            for fix in fixes {
                d = d.with_fix(fix.clone());
            }
            d
        }
        _ => unreachable!("not one of what can't be seen from where it's used"),
    }
}

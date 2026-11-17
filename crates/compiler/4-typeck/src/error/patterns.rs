use som_base::diagnostics::closest_match;
use som_base::message::{Message, code, field, struct_name};
use som_base::{Diagnostic, Label, msg};

use super::{DiagnosticExt, TypeError};

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::UnknownVariant {
            ty: owner,
            variant,
            available,
            span,
        } => {
            let mut d =
                Diagnostic::error(msg![struct_name(owner), " has no variant ", code(variant)])
                    .with_label(Label::primary(span.clone(), "unknown variant"));
            if let Some(close) = closest_match(variant, available, 2) {
                d = d.with_hint(msg!["did you mean ", code(close), "?"]);
            } else if !available.is_empty() {
                let mut hint = msg!["its variants: "];
                for (i, v) in available.iter().enumerate() {
                    if i > 0 {
                        hint.0.push(", ".into());
                    }
                    hint.0.push(code(v));
                }
                d = d.with_hint(hint);
            }
            d
        }
        TypeError::RustOptionalPattern {
            written,
            span,
            fix,
            needs_none_first,
        } => {
            let (label, hint) = match written.as_str() {
                "None" => (
                    msg!["an optional without a value is ", code("none")],
                    msg!["a capitalised name here would be a new variable, matching anything"],
                ),
                _ if *needs_none_first => (
                    msg!["an optional's value is matched by the pattern itself"],
                    msg![
                        "match ",
                        code("none"),
                        " first; after it, a name holds the value: ",
                        code("none => …, v => …")
                    ],
                ),
                _ => (
                    msg!["an optional's value is matched by the pattern itself"],
                    msg![
                        code("5"),
                        " or ",
                        code("Circle(r)"),
                        " matches an optional holding it; ",
                        code("none"),
                        " one without"
                    ],
                ),
            };
            let d = Diagnostic::error(msg!["som has no ", code(written), " pattern"])
                .with_label(Label::primary(span.clone(), label))
                .with_hint(hint);
            match fix {
                Some(fix) => d.with_fix(fix.clone()),
                None => d,
            }
        }
        TypeError::PatternMismatch { pattern, ty, span } => {
            // `ty` brings its own article when it's in words: "a string", "an integer".
            Diagnostic::error(msg![code(pattern), " can't match ", ty.clone()]).with_label(
                Label::primary(span.clone(), msg![ty.clone(), " is matched here"]),
            )
        }
        TypeError::PatternFieldCount {
            variant,
            expected,
            found,
            span,
        } => {
            let values = |n: usize| if n == 1 { "value" } else { "values" };
            Diagnostic::error(msg![
                code(variant),
                format!(" holds {expected} {}", values(*expected))
            ])
            .with_label(Label::primary(
                span.clone(),
                format!("{found} {} matched here", values(*found)),
            ))
        }
        TypeError::MissingFieldPatterns {
            variant,
            fields,
            span,
        } => {
            let mut message = msg![code(variant), " also has "];
            message.push(Message::list(fields.iter().map(field), " and "));
            Diagnostic::error(message)
                .with_label(Label::primary(span.clone(), "not matched here"))
                .with_hint(msg![
                    "match them too, or add ",
                    code(".."),
                    " to ignore them"
                ])
        }
        TypeError::NonExhaustiveMatch {
            ty,
            missing,
            span,
            fix,
        } => {
            // Only a `_` covers the rest of a number.
            if missing.len() == 1 && missing[0] == "_" {
                let d = Diagnostic::error(msg!["not every ", ty.clone(), " is matched"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg!["add a ", code("_"), " arm for the rest"],
                    ));
                return match fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d,
                };
            }
            let mut message = Message::list(missing.iter().map(code), " and ");
            let verb = if missing.len() == 1 {
                " isn't"
            } else {
                " aren't"
            };
            message.push(msg![verb, " matched"]);
            let what = if missing.len() == 1 {
                "that"
            } else {
                "any of those"
            };
            Diagnostic::error(message)
                .with_label(Label::primary(
                    span.clone(),
                    msg!["this ", ty.clone(), " can be ", what],
                ))
                .with_optional_fix(fix)
        }
        TypeError::UnreachableArm { span, earlier } => {
            let d = Diagnostic::error("this arm is never reached")
                .with_label(Label::primary(span.clone(), "already matched above"));
            match earlier {
                Some(earlier) => d.with_label(Label::secondary(
                    earlier.clone(),
                    "this arm matches everything",
                )),
                None => d,
            }
        }
        TypeError::WrongVariantForm {
            name,
            holds,
            form,
            span,
        } => Diagnostic::error(msg![code(name), " ", *holds])
            .with_label(Label::primary(span.clone(), msg!["write ", code(form)])),
        TypeError::ConstructEnum {
            name,
            example,
            span,
        } => {
            let d = Diagnostic::error(msg![struct_name(name), " is an enum"]).with_label(
                Label::primary(span.clone(), "make one of its variants instead"),
            );
            match example {
                Some(example) => d.with_hint(msg!["for example ", code(example)]),
                None => d,
            }
        }
        _ => unreachable!("not one of `match`es and their patterns"),
    }
}

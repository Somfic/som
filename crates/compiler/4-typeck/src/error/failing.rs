use som_base::message::code;
use som_base::{Diagnostic, Fix, Label, msg};

use super::TypeError;

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::FailInInfallible {
            what,
            gives,
            span,
            returns,
        } => {
            let d = Diagnostic::error(msg![code(*what), " in a function that can't fail"])
                .with_label(Label::primary(span.clone(), msg!["the function gives ", gives.clone()]));
            let d = match returns {
                Some(at) => d.with_label(Label::secondary(at.clone(), "said here")),
                None => d,
            };
            d.with_hint(msg![
                "a function that can fail gives a ",
                code("T!"),
                ": ",
                code("-> i32!"),
                ", or ",
                code("-> !"),
                " for one that gives nothing"
            ])
        }
        TypeError::NotFailable {
            what,
            ty,
            optional,
            span,
        } => {
            let d = Diagnostic::error(msg![code(*what), " of what can't fail"])
                .with_label(Label::primary(span.clone(), msg!["this is ", ty.clone(), ", which can't fail"]));
            match (optional, *what) {
                (true, "?") => d.with_hint(msg![
                    "for an optional, give a fallback for ",
                    code("none"),
                    ": ",
                    code("value else return none")
                ]),
                _ => d,
            }
        }
        TypeError::FailureUnhandled {
            ty,
            span,
            expected_from,
        } => {
            let d = Diagnostic::error(msg!["this can fail, where a value is expected"])
                .with_label(Label::primary(span.clone(), msg!["this is ", ty.clone(), ", which can fail"]));
            let d = match expected_from {
                Some(from) => d.with_label(Label::secondary(from.clone(), "a value expected because of this")),
                None => d,
            };
            d.with_hint(msg![
                code("?"),
                " gives the value, or passes the failure on; ",
                code("else …"),
                " gives something in its place"
            ])
            .with_fix(Fix::replace(msg!["pass it on: ", code("?")], span.after(), "?"))
        }
        TypeError::FailsUndeclared {
            func,
            kinds,
            span,
            said,
        } => {
            let what = match &kinds[..] {
                [one] if one == "…" => msg!["something not known here"],
                [one] => msg!["a ", code(one)],
                _ => msg![code(kinds.join(", "))],
            };
            let d = Diagnostic::error(msg![code(func), " can fail with ", what.clone(), ", which it doesn't say"])
                .with_label(Label::primary(span.clone(), msg!["this can fail with ", what]));
            let d = match said {
                Some(at) => d.with_label(Label::secondary(at.clone(), "what it fails with is said here")),
                None => d,
            };
            d.with_hint(msg![
                "handle it here with ",
                code("else"),
                ", fail with one of the types it says, or add it to them"
            ])
        }
        TypeError::CantFailWith { kind, fails, span } => {
            let d = Diagnostic::error(msg!["this can't fail with a ", code(kind)])
                .with_label(Label::primary(span.clone(), "no failure gets to this arm"));
            match fails == "()" {
                true => d.with_hint("what's matched never fails"),
                false => d.with_hint(msg!["it fails with ", code(fails)]),
            }
        }
        TypeError::FailedPatternNotAType { span } => {
            Diagnostic::error(msg!["after ", code("fail"), " in a pattern, a name or a type"])
                .with_label(Label::primary(span.clone(), "this matches neither"))
                .with_hint(msg![
                    code("fail e"),
                    " takes any failure; ",
                    code("fail NotFound { path }"),
                    " one with a ",
                    code("NotFound")
                ])
        }
        _ => unreachable!("not a failing error"),
    }
}

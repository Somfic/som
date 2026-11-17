use som_base::message::{Message, code, field, func, struct_name, var};
use som_base::{Diagnostic, Fix, Label, msg};

use super::{TypeError, member_hints};

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::WrongArgCount {
            name,
            expected,
            at_least,
            found,
            span,
        } => {
            let arguments = |n: usize| if n == 1 { "argument" } else { "arguments" };
            let given = if *found == 1 { "was" } else { "were" };
            let count = if *at_least {
                format!("at least {expected}")
            } else {
                expected.to_string()
            };
            Diagnostic::error(msg![
                func(name),
                format!(
                    " takes {count} {}, but {found} {given} given",
                    arguments(*expected)
                ),
            ])
            .with_label(Label::primary(
                span.clone(),
                format!("expected {count} {}", arguments(*expected)),
            ))
        }
        TypeError::UnknownField {
            ty: owner,
            field: name,
            available,
            props,
            methods,
            span,
        } => {
            let d = Diagnostic::error(msg![struct_name(owner), " has no field ", field(name)])
                .with_label(Label::primary(span.clone(), "unknown field"));
            member_hints(d, name, span, None, false, available, props, methods)
        }
        TypeError::MissingField {
            ty: owner,
            field: name,
            span,
        } => Diagnostic::error(msg![
            "missing field ",
            field(name),
            " in ",
            struct_name(owner)
        ])
        .with_label(Label::primary(
            span.clone(),
            msg![field(name), " is not given a value"],
        )),
        TypeError::DuplicateField { field: name, span } => {
            Diagnostic::error(msg!["field ", field(name), " is given twice"])
                .with_label(Label::primary(span.clone(), "second value here"))
        }
        TypeError::UnknownMethod {
            ty: owner,
            method,
            span,
            methods,
            props,
            fields,
            bare_call,
        } => {
            let d = Diagnostic::error(msg![owner.clone(), " has no method ", func(method)])
                .with_label(Label::primary(span.clone(), "unknown method"));
            member_hints(
                d,
                method,
                span,
                bare_call.as_ref(),
                true,
                fields,
                props,
                methods,
            )
        }
        TypeError::NotAFunction {
            name,
            ty: found,
            span,
        } => Diagnostic::error(msg![var(name), " is not a function"]).with_label(Label::primary(
            span.clone(),
            msg!["this is ", found.clone()],
        )),
        TypeError::NoMatchingOverload {
            name,
            args,
            candidates,
            span,
        } => {
            let mut given = Message(Vec::new());
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    given.0.push(", ".into());
                }
                given.push(a.clone());
            }
            let label = match args.is_empty() {
                true => msg!["called with no arguments"],
                false => msg!["called with ", given],
            };
            let mut d = Diagnostic::error(msg!["no ", func(name), " fits this call"])
                .with_label(Label::primary(span.clone(), label));
            for (c, at) in candidates {
                d = d.with_label(Label::secondary(at.clone(), msg!["candidate ", code(c)]));
            }
            d
        }
        TypeError::AmbiguousCall {
            name,
            candidates,
            span,
        } => {
            let mut d = Diagnostic::error(msg!["ambiguous call to ", func(name)]).with_label(
                Label::primary(span.clone(), "cannot determine which overload to use"),
            );
            for (c, at) in candidates {
                d = d.with_label(Label::secondary(at.clone(), msg!["candidate ", code(c)]));
            }
            d.with_hint("specify a return type to indicate which overload to use")
        }
        TypeError::DuplicateOverload {
            name,
            span,
            previous,
        } => Diagnostic::error(msg![
            func(name),
            " is already defined with the same signature"
        ])
        .with_label(Label::primary(span.clone(), "and is defined again here"))
        .with_label(Label::secondary(
            previous.clone(),
            "was already defined here",
        )),
        TypeError::MutOverload {
            name,
            span,
            previous,
            fix,
        } => Diagnostic::error(msg![
            func(name),
            " is defined twice, differing only in ",
            code("&"),
            " and ",
            code("&mut")
        ])
        .with_label(Label::primary(span.clone(), "the one that changes"))
        .with_label(Label::secondary(previous.clone(), "the one that reads"))
        .with_hint(msg![
            "which one runs would depend on whether the caller has a ",
            code("&mut"),
            ": give the one that changes its own name"
        ])
        .with_fix(fix.clone()),
        TypeError::FieldIsMethod { ty, name, span } => {
            Diagnostic::error(msg![ty.clone(), " has no field ", code(name.clone())])
                .with_label(Label::primary(span.clone(), "it's a method"))
                .with_fix(Fix {
                    title: msg!["call it: ", code(format!("{name}()"))],
                    edits: vec![(span.after(), "()".to_string())],
                })
        }
        TypeError::MethodIsProp {
            ty,
            name,
            span,
            parens,
        } => {
            let diagnostic =
                Diagnostic::error(msg![ty.clone(), " has no method ", func(name.clone())])
                    .with_label(Label::primary(span.clone(), "it's a property"));
            match parens {
                Some(parens) => diagnostic.with_fix(Fix {
                    title: msg!["read it without parentheses: ", code(name.clone())],
                    edits: vec![(parens.clone(), String::new())],
                }),
                None => diagnostic
                    .with_hint(msg!["a property takes no arguments: ", code(name.clone())]),
            }
        }
        TypeError::PropReturnsNothing { name, span } => {
            Diagnostic::error(msg!["property ", func(name.clone()), " returns nothing"])
                .with_label(Label::primary(
                    span.clone(),
                    "a property has to give a value",
                ))
                .with_hint(msg![
                    "one that only does something is a method: ",
                    code(format!("fn {name}"))
                ])
        }
        TypeError::NotOnImplTarget {
            ty,
            what,
            name,
            targets,
            span,
        } => {
            let mut only = msg![code(name.clone()), " is only defined for "];
            only.push(Message::list(
                targets.iter().map(|(target, _)| code(target)),
                " and ",
            ));
            let mut diagnostic =
                Diagnostic::error(msg![ty.clone(), " has no ", *what, " ", func(name.clone())])
                    .with_label(Label::primary(span.clone(), only));
            for (_, at) in targets {
                diagnostic =
                    diagnostic.with_label(Label::secondary(at.clone(), "defined for this"));
            }
            diagnostic
        }
        TypeError::NeedsOut {
            param,
            span,
            declared_at,
        } => Diagnostic::error(msg![var(param), " is an ", code("out"), " parameter"])
            .with_label(Label::primary(
                span.clone(),
                msg!["pass ", code(format!("out {param}")), " to get its value"],
            ))
            .with_label(Label::secondary(declared_at.clone(), "declared here")),
        TypeError::MainReturns { ty, span, written } => {
            let d = Diagnostic::error(msg![
                code(som_lang::ENTRY_POINT),
                " can't give ",
                ty.clone()
            ])
            .with_label(Label::primary(
                span.clone(),
                "what it gives is the program's exit code: an integer, or nothing",
            ));
            match written {
                true => d,
                // The script's last line: a value, not a statement.
                false => d
                    .with_hint(msg![
                        "a program's last value is its exit code; to write it out, use ",
                        code("println")
                    ])
                    .with_fix(Fix::replace(
                        msg!["end it with ", code(";")],
                        span.after(),
                        ";",
                    )),
            }
        }
        TypeError::MainParams { span } => {
            Diagnostic::error(msg![code(som_lang::ENTRY_POINT), " takes no parameters"])
                .with_label(Label::primary(
                    span.clone(),
                    "the program's arguments aren't passed here",
                ))
                .with_hint(msg![
                    "read them with ",
                    code("args()"),
                    " from ",
                    code("som::io"),
                    ": the program's path, then each argument"
                ])
                .with_fix(Fix::replace(msg!["take nothing"], span.clone(), ""))
        }
        TypeError::ListenerSignature { span } => {
            Diagnostic::error(msg!["an ", code("@on fail"), " function takes what failed"])
                .with_label(Label::primary(
                    span.clone(),
                    "called when the program fails",
                ))
                .with_hint(msg!["write it ", code("fn name(failure: &Failure)")])
        }
        TypeError::TestSignature { span } => Diagnostic::error(msg![
            "a ",
            code("@test"),
            " takes nothing and gives nothing back"
        ])
        .with_label(Label::primary(span.clone(), "called by the test runner"))
        .with_hint(msg![
            "write it ",
            code("fn name()"),
            ", and check what it needs with ",
            code("assert")
        ]),
        TypeError::ConstructHandle { name, span } => {
            Diagnostic::error(msg!["cannot create a ", struct_name(name), " in som"])
                .with_label(Label::primary(
                    span.clone(),
                    msg![
                        "it's an ",
                        code("extern struct"),
                        ": only C knows what's inside"
                    ],
                ))
                .with_hint("get one from the C library, or zeroed with `out`")
        }
        TypeError::NotInEveryVariant {
            ty: owner,
            field: name,
            without,
            span,
        } => {
            let label = match without.as_slice() {
                [] => msg!["its type differs between the variants"],
                [one] => msg![code(one), " doesn't have it"],
                many => {
                    let mut m = Message::list(many.iter().map(code), " and ");
                    m.push(" don't have it");
                    m
                }
            };
            Diagnostic::error(msg![
                field(name),
                " isn't in every variant of ",
                struct_name(owner)
            ])
            .with_label(Label::primary(span.clone(), label))
            .with_hint(msg![
                "use ",
                code("match"),
                " to reach it in the variants that have it"
            ])
        }
        _ => unreachable!("not one of calls, and the members they name"),
    }
}

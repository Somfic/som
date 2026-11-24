use som_base::message::{Message, code};
use som_base::{Diagnostic, Fix, Label, msg};
use som_lang::LangTrait;

use super::{DiagnosticExt, NotForC, TypeError, Unused, optional_hints};

pub(super) fn diagnostic(error: &TypeError) -> Diagnostic {
    match error {
        TypeError::NotBorrowed {
            expected,
            found,
            field,
            span,
            expected_from,
            fix,
        } => {
            let label = if *field {
                msg![
                    "this is the field itself, ",
                    found.clone(),
                    "; a reference to it is expected"
                ]
            } else {
                msg![
                    "this is ",
                    found.clone(),
                    "; a reference to one is expected"
                ]
            };
            Diagnostic::error(msg![
                "expected ",
                expected.clone(),
                ", found ",
                found.clone()
            ])
            .with_label(Label::primary(span.clone(), label))
            .expected_from(expected_from, expected)
            .with_fix(fix.clone())
        }
        TypeError::Mismatch {
            expected,
            found,
            span,
            expected_from,
            fix,
            optional,
        } => {
            let d = Diagnostic::error(msg![
                "expected ",
                expected.clone(),
                ", found ",
                found.clone()
            ])
            .with_label(Label::primary(
                span.clone(),
                msg!["this is ", found.clone()],
            ))
            .expected_from(expected_from, expected)
            .with_optional_fix(fix);
            match optional {
                Some((object, variable)) => optional_hints(d, object, *variable),
                None => d,
            }
        }
        TypeError::InvalidOperands {
            op,
            lhs,
            rhs,
            span,
            optional,
            implement,
            fix,
            takes,
        } => {
            let d = Diagnostic::error(msg![
                "cannot apply ",
                code(op),
                " to ",
                lhs.clone(),
                " and ",
                rhs.clone()
            ]);
            match optional {
                Some((name, fallback, at)) => d
                    .with_label(Label::primary(
                        at.clone(),
                        msg![code(name), " might be ", code("none"), " here"],
                    ))
                    .with_hint(msg![
                        "check it first, as in ",
                        code(format!("if {name} {{ … }}")),
                        ", or give a fallback, as in ",
                        code(format!("({name} else {fallback})")),
                    ]),
                None if let Some((trait_, param)) = implement => {
                    let d = d
                        .with_label(Label::primary(span.clone(), "no such operation"))
                        .with_optional_fix(fix);
                    match param {
                        Some(param) => d.with_hint(msg![
                            "to use ",
                            code(op),
                            ", require it: ",
                            code(format!("{param}: {trait_}"))
                        ]),
                        None => d.with_hint(msg![
                            "to give it ",
                            code(op),
                            ", implement ",
                            code(*trait_),
                            " for the type"
                        ]),
                    }
                }
                // `"ab" * 3`: it has the operator, with something else on the right.
                None if !takes.is_empty() => {
                    let mut hint = msg![lhs.clone(), " has ", code(op), " with "];
                    hint.push(Message::list(takes.iter().map(code), " or "));
                    hint.push(" on the right");
                    let d = d
                        .with_label(Label::primary(span.clone(), "no such operation"))
                        .with_hint(hint);
                    match fix {
                        Some(fix) => d.with_fix(fix.clone()),
                        None => d,
                    }
                }
                None => {
                    let d = d.with_label(Label::primary(span.clone(), "no such operation"));
                    match fix {
                        Some(fix) => d.with_fix(fix.clone()),
                        None => d,
                    }
                }
            }
        }
        TypeError::CannotDeref { ty: found, span } => {
            Diagnostic::error(msg!["cannot dereference ", found.clone()]).with_label(
                Label::primary(span.clone(), "only references can be dereferenced"),
            )
        }
        TypeError::NotAStruct { ty: found, span } => {
            Diagnostic::error(msg![found.clone(), " is not a struct"]).with_label(Label::primary(
                span.clone(),
                "only structs have fields and methods",
            ))
        }
        TypeError::ReferenceToReference { object, ty, span } => {
            Diagnostic::error(msg![code(object.clone()), " is a reference to a reference"])
                .with_label(Label::primary(
                    span.clone(),
                    msg![
                        "this is ",
                        ty.clone(),
                        ": fields are reached through one reference"
                    ],
                ))
                .with_hint(msg![
                    "it was likely made with ",
                    code("&"),
                    " or ",
                    code("&mut"),
                    " of something that was a reference already: leave that out"
                ])
                .with_hint(msg![
                    "to get a mutable reference from a call, give the variable its type: ",
                    code("let x: &mut T = …")
                ])
        }
        TypeError::MemberOfOptional {
            object,
            ty,
            variable,
            span,
        } => {
            let d = Diagnostic::error(msg![code(object.clone()), " might be ", code("none")])
                .with_label(Label::primary(
                    span.clone(),
                    msg![
                        "this is ",
                        ty.clone(),
                        ": check it holds a value before using it"
                    ],
                ));
            optional_hints(d, object, *variable)
        }
        TypeError::StrIsString { span } => Diagnostic::error(msg![
            "som's text is ",
            code(som_lang::LangStruct::String.name())
        ])
        .with_label(Label::primary(
            span.clone(),
            msg!["text is ", code(som_lang::LangStruct::String.name())],
        ))
        .with_fix(Fix::change_to(
            span.clone(),
            som_lang::LangStruct::String.name(),
        )),
        TypeError::NotForC { ty, what, span } => {
            let d =
                Diagnostic::error(msg!["C can't take ", ty.clone()]).with_label(Label::primary(
                    span.clone(),
                    "C reads it as the number or pointer its format says",
                ));
            let text = span.get_text();
            match what {
                NotForC::Optional => d.with_hint(msg![
                    "C has no optionals: check it first, as in ",
                    code(format!("if {text} {{ … }}")),
                    ", or give a fallback, as in ",
                    code(format!("({text} else 0)"))
                ]),
                NotForC::Reference => d.with_hint("C takes what it refers to").with_fix(Fix {
                    title: msg!["pass ", code(format!("*{text}"))],
                    edits: vec![(span.before(), "*".to_string())],
                }),
                NotForC::Other => d.with_hint(msg![
                    "C takes numbers, pointers and ",
                    code("cstr"),
                    " here; to print it, use ",
                    code("println(\"{…}\")")
                ]),
            }
        }
        TypeError::StrToC { span } => {
            let d = Diagnostic::error(msg![
                "C can't take a ",
                code(som_lang::LangStruct::String.name())
            ])
            .with_label(Label::primary(span.clone(), "C takes a `cstr`"))
            .with_hint(msg![
                "pass its ",
                code(".c_str"),
                ", valid as long as the string is; a literal becomes a ",
                code("cstr"),
                " by itself"
            ]);
            // A name, or a path through fields: `.c_str` goes straight after it.
            let text = span.get_text();
            let plain = text
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
            if plain {
                d.with_fix(Fix {
                    title: msg!["pass ", code(format!("{text}.c_str"))],
                    edits: vec![(span.after(), ".c_str".to_string())],
                })
            } else {
                d
            }
        }
        TypeError::MissingReturn { ty, span, returns } => {
            let d = Diagnostic::error(msg!["not every path returns ", ty.clone()]).with_label(
                Label::primary(span.clone(), "this end can be reached without a value"),
            );
            let d = match returns {
                Some(at) => d.with_label(Label::secondary(at.clone(), "declared to return this")),
                None => d,
            };
            d.with_hint(msg![
                "end every path in ",
                code("return"),
                ", or end the function with a value"
            ])
        }
        TypeError::ReturnWithoutValue { ty, span } => {
            Diagnostic::error(msg![code("return"), " needs a value here"]).with_label(
                Label::primary(span.clone(), msg!["the function returns ", ty.clone()]),
            )
        }
        TypeError::LiteralOutOfRange {
            value,
            ty,
            max,
            min,
            span,
            type_arg,
        } => {
            let limit = match *value < 0 {
                true => msg!["the smallest ", code(ty), " is ", code(min.to_string())],
                false => msg!["the largest ", code(ty), " is ", code(max.to_string())],
            };
            let error =
                Diagnostic::error(msg![code(value.to_string()), " doesn't fit in ", code(ty)])
                    .with_label(Label::primary(span.clone(), limit));
            match type_arg {
                true => error,
                false => error.with_hint(msg![
                    "to wrap it on purpose, convert it: ",
                    code(format!("{value} as {ty}"))
                ]),
            }
        }
        TypeError::UnitTail { span } => Diagnostic::error("expected `;`")
            .with_label(Label::primary(
                span.after(),
                "expected a semicolon after the call",
            ))
            .with_fix(Fix::replace(msg!["add the ", code(";")], span.after(), ";")),
        TypeError::CannotNegate { ty, span } => {
            Diagnostic::error(msg!["cannot negate ", ty.clone()]).with_label(Label::primary(
                span.clone(),
                "only signed integers and floats have a `-`",
            ))
        }
        TypeError::CannotFlipBits { ty, span } => {
            Diagnostic::error(msg!["cannot flip the bits of ", ty.clone()])
                .with_label(Label::primary(span.clone(), "only integers have a `~`"))
        }
        TypeError::InvalidCast { from, to, span } => {
            Diagnostic::error(msg!["cannot convert ", from.clone(), " to ", to.clone()]).with_label(
                Label::primary(span.clone(), "only numbers convert with `as`"),
            )
        }
        TypeError::NotIterable { ty, span, text } => {
            let d = Diagnostic::error(msg!["cannot loop over ", ty.clone()]).with_label(
                Label::primary(
                    span.clone(),
                    msg![
                        code("for"),
                        " goes through ranges, arrays, slices and what implements ",
                        code(LangTrait::IterRef.name()),
                        " or ",
                        code(LangTrait::Iterator.name())
                    ],
                ),
            );
            if *text {
                d.with_hint(msg![
                    code("bytes()"),
                    " gives the text's bytes to loop over"
                ])
            } else {
                d
            }
        }
        TypeError::OpenRange { span } => Diagnostic::error("this range needs both ends")
            .with_label(Label::primary(
                span.clone(),
                "where it starts or stops isn't known here",
            ))
            .with_hint(msg![
                "only ",
                code("slice"),
                " knows where a range like ",
                code("1.."),
                " ends"
            ]),
        TypeError::DerefSlice { span } => Diagnostic::error("cannot dereference a slice")
            .with_label(Label::primary(
                span.clone(),
                "how many elements it views isn't fixed",
            ))
            .with_hint(msg!["use its methods instead, like ", code("get")]),
        TypeError::NotCopyable { ty, span, repeat } => {
            let (message, hint) = if *repeat {
                (
                    msg!["cannot make copies of ", ty.clone()],
                    msg!["write each element out, as in ", code("[a, b, c]")],
                )
            } else {
                (
                    msg!["cannot copy ", ty.clone(), " out of the array"],
                    msg![code("get_ref"), " borrows the element instead"],
                )
            };
            Diagnostic::error(message)
                .with_label(Label::primary(
                    span.clone(),
                    "its values move rather than copy",
                ))
                .with_hint(hint)
        }
        TypeError::OptionalBoolCondition { span } => {
            Diagnostic::error(msg!["cannot use a ", code("bool?"), " as a condition"])
                .with_label(Label::primary(
                    span.clone(),
                    "this could mean it holds a value, or that it's true",
                ))
                .with_hint(msg![
                    "write ",
                    code("!= none"),
                    " to check for a value, or ",
                    code("== true"),
                    " to check that it's true"
                ])
        }
        TypeError::NotOptional { ty, span } => {
            Diagnostic::error(msg!["expected an optional, found ", ty.clone()]).with_label(
                Label::primary(
                    span.clone(),
                    msg!["this is ", ty.clone(), ", which always has a value"],
                ),
            )
        }
        TypeError::NoneAsPointer {
            ty,
            span,
            expected_from,
        } => Diagnostic::error(msg![code("none"), " isn't a pointer"])
            .with_label(Label::primary(
                span.clone(),
                msg![ty.clone(), " expected, which ", code("none"), " can't be"],
            ))
            .expected_from(expected_from, ty)
            .with_hint(msg![
                code("none"),
                " is an optional without a value; a null pointer is ",
                code("som::mem::null()")
            ])
            .with_fix(Fix::change_to(span.clone(), "som::mem::null()")),
        TypeError::CannotInfer { span } => {
            Diagnostic::error("cannot infer the type of this expression")
                .with_label(Label::primary(span.clone(), "add a type annotation"))
        }
        TypeError::IndexNotArray { ty, span, get } => {
            Diagnostic::error(msg!["cannot index ", ty.clone(), " with ", code("[]")])
                .with_label(Label::primary(
                    span.clone(),
                    msg![
                        "not an array, and it doesn't implement ",
                        code(LangTrait::Index.name())
                    ],
                ))
                .with_hint(msg!["use ", code(get.clone()), ", which gives an optional"])
        }
        TypeError::IndexOutOfBounds { index, len, span } => {
            Diagnostic::error(msg!["index ", code(index.to_string()), " is out of bounds"])
                .with_label(Label::primary(
                    span.clone(),
                    format!(
                        "the array has {len} element{}",
                        if *len == 1 { "" } else { "s" }
                    ),
                ))
        }
        TypeError::IndexUnknownLength { len, span, get } => {
            Diagnostic::error("cannot tell this index is in bounds")
                .with_label(Label::primary(
                    span.clone(),
                    msg![
                        "the array's length, ",
                        code(len.clone()),
                        ", isn't known here"
                    ],
                ))
                .with_hint(msg!["use ", code(get.clone()), ", which gives an optional"])
        }
        TypeError::UnusedValue { kind, span } => match kind {
            Unused::Comparison(assignment) => {
                Diagnostic::error("the comparison's result is unused")
                    .with_label(Label::primary(
                        span.clone(),
                        "this compares, it doesn't assign",
                    ))
                    .with_fix(Fix::replace(
                        msg!["assign instead: ", code(assignment.clone())],
                        span.clone(),
                        assignment.clone(),
                    ))
            }
            Unused::Failure => Diagnostic::error("this can fail, and nothing looks at whether it did")
                .with_label(Label::primary(span.clone(), "a failure here would go unseen"))
                .with_hint(msg![
                    code("?"),
                    " passes it on; ",
                    code("else …"),
                    " says what to do instead"
                ])
                .with_fix(Fix::replace(msg!["pass it on: ", code("?")], span.after(), "?")),
            Unused::Value => Diagnostic::error("this value is unused")
                .with_label(Label::primary(
                    span.clone(),
                    "working it out does nothing else",
                ))
                .with_hint(msg![
                    "use it, or if dropping it is the point, ",
                    code("drop(…)"),
                    " it"
                ]),
        },
        TypeError::BreakOutsideLoop { keyword, span } => {
            Diagnostic::error(msg![code(*keyword), " outside of a loop"]).with_label(
                Label::primary(
                    span.clone(),
                    msg![
                        "only inside a ",
                        code("loop"),
                        ", ",
                        code("while"),
                        " or ",
                        code("for")
                    ],
                ),
            )
        }
        _ => unreachable!("not one of values of the wrong type"),
    }
}

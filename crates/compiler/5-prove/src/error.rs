use som_base::message::code;
use som_base::{Diagnostic, Fix, Label, Message, Span, msg};

#[derive(Debug, Clone)]
pub enum ProveError {
    /// `xs[i]` where `i` isn't known to be below `xs.len`.
    MaybeOutOfBounds {
        /// It's known to be outside, rather than not known to be inside.
        never: bool,
        /// The index and what it has to be below, as written: `i`, `xs.len`.
        index: String,
        len: String,
        /// The index is a number written out, `xs[2]`.
        written: bool,
        /// What is known: `xs.len` is only known to be at least 3.
        known: Message,
        span: Span,
        /// Using `xs.get(i)` instead, where it's read.
        get_fix: Option<Fix>,
        /// The check that would do: `i < xs.len`.
        check: String,
        fix: Option<Fix>,
    },
    /// A call of a function whose `@where` isn't known to hold there.
    Unproven {
        /// The function, and its condition as it reads at the call: `k < xs.len`.
        func: String,
        condition: String,
        /// What it means, if its `@where` says.
        reason: Option<String>,
        /// It's known not to hold, rather than not known to.
        never: bool,
        span: Span,
        /// The `@where` it's from.
        declared: Span,
        fix: Option<Fix>,
    },
    /// `a / b` or `a % b` of integers where `b` isn't known not to be 0.
    MaybeZeroDivisor {
        /// The divisor is a `0` written out.
        written: bool,
        /// The divisor as written, and the check that would do: `n - 5`, `(n - 5) != 0`.
        divisor: String,
        check: String,
        /// It's known to be 0, rather than not known not to be.
        never: bool,
        span: Span,
        fix: Option<Fix>,
    },
    /// A `@where` the prover can't follow: one that calls a function, say.
    UnfollowedWhere { span: Span },
    /// A `@where` on a trait's method in an implementation: code calling it through the
    /// trait only knows the trait's.
    WhereOnImpl {
        method: String,
        trait_: String,
        span: Span,
    },
}

impl ProveError {
    pub fn span(&self) -> &Span {
        match self {
            ProveError::MaybeOutOfBounds { span, .. }
            | ProveError::Unproven { span, .. }
            | ProveError::MaybeZeroDivisor { span, .. }
            | ProveError::UnfollowedWhere { span }
            | ProveError::WhereOnImpl { span, .. } => span,
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            ProveError::MaybeOutOfBounds {
                never,
                index,
                len,
                written,
                known,
                span,
                get_fix,
                check,
                fix,
            } => {
                let d = Diagnostic::error(msg![
                    "index ",
                    code(index.clone()),
                    match never {
                        true => " is out of bounds",
                        false => " is possibly out of bounds",
                    }
                ])
                .with_label(Label::primary(span.clone(), known.clone()))
                .with_hint(match written {
                    true => msg![
                        code(len.clone()),
                        " is required to be more than ",
                        code(index.clone()),
                        " here, but it cannot be proven"
                    ],
                    false => msg![
                        code(index.clone()),
                        " is required be below ",
                        code(len.clone()),
                        " here, but it connect be proven"
                    ],
                });
                // Where the check can't be put in for you: what to write.
                let d = match fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d.with_hint(match written {
                        true => msg![
                            "check the length first: ",
                            code(format!("if {check} {{ … }}"))
                        ],
                        false => msg!["check it first: ", code(format!("if {check} {{ … }}"))],
                    }),
                };
                let d = match written {
                    true => d,
                    false => d.with_hint(msg![
                        "or loop over the positions: ",
                        code(format!("for {index} in 0..{len}"))
                    ]),
                };
                match get_fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d,
                }
            }
            ProveError::MaybeZeroDivisor {
                written: true,
                span,
                ..
            } => Diagnostic::error("dividing by zero")
                .with_label(Label::primary(span.clone(), "this is 0"))
                .with_hint("dividing by zero has no answer"),
            ProveError::MaybeZeroDivisor {
                written: false,
                divisor,
                check,
                never,
                span,
                fix,
            } => {
                let d = Diagnostic::error(match never {
                    true => msg!["dividing by zero: ", code(divisor.clone()), " is 0 here"],
                    false => msg!["dividing by ", code(divisor.clone()), ", which might be 0"],
                })
                .with_label(Label::primary(
                    span.clone(),
                    match never {
                        true => msg!["this is 0"],
                        false => msg!["not known not to be 0"],
                    },
                ))
                .with_hint(msg![
                    "dividing by zero has no answer, and nothing is checked as the program \
                     runs, so ",
                    code(divisor.clone()),
                    " has to be known not to be 0 here"
                ]);
                match fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d.with_hint(msg![
                        "check it first: ",
                        code(format!("if {check} {{ … }}"))
                    ]),
                }
            }
            ProveError::WhereOnImpl {
                method,
                trait_,
                span,
            } => Diagnostic::error(msg![
                code(method.clone()),
                " can't add a ",
                code("@where"),
                " to the one ",
                code(trait_.clone()),
                " gives it"
            ])
            .with_label(Label::primary(span.clone(), "added here"))
            .with_hint(msg![
                "code calling it through ",
                code(trait_.clone()),
                " only knows the trait's conditions, so it couldn't make sure of this one"
            ])
            .with_hint(msg![
                "check it inside ",
                code(method.clone()),
                " instead, or put it on ",
                code(trait_.clone()),
                "'s ",
                code(method.clone()),
                " for every implementation"
            ]),
            ProveError::UnfollowedWhere { span } => {
                Diagnostic::error(msg!["this ", code("@where"), " can't be checked"])
                    .with_label(Label::primary(span.clone(), "not followed"))
                    .with_hint(msg![
                        "a condition compares integers: parameters, their fields and properties, \
                         and numbers, with ",
                        code("<"),
                        ", ",
                        code("<="),
                        ", ",
                        code("=="),
                        ", ",
                        code("!="),
                        ", and ",
                        code("&&"),
                        "; or says a ",
                        code("bool"),
                        " property is true or false"
                    ])
            }
            ProveError::Unproven {
                func,
                condition,
                reason,
                never,
                span,
                declared,
                fix,
            } => {
                let title = match never {
                    true => msg![code(condition.clone()), " is false here"],
                    false => msg!["can't tell ", code(condition.clone()), " holds here"],
                };
                let d = Diagnostic::error(title)
                    .with_label(Label::primary(
                        span.clone(),
                        match reason {
                            Some(reason) => msg![code(func.clone()), " needs it: ", reason.clone()],
                            None => msg![code(func.clone()), " needs it"],
                        },
                    ))
                    .with_hint(msg![
                        code(func.clone()),
                        " only works where ",
                        code(declared.get_text().to_string()),
                        ", so it has to be known here"
                    ]);
                match fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d.with_hint(msg![
                        "check it first: ",
                        code(format!("if {condition} {{ … }}"))
                    ]),
                }
            }
        }
    }
}

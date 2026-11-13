use som_base::diagnostics::closest_match;
use som_base::message::{Message, code, field, func, struct_name, var};
use som_base::{Diagnostic, Fix, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum TypeError {
    /// `found` was used where `expected` is required. `expected_from` points at what required it.
    Mismatch {
        expected: Message,
        found: Message,
        span: Span,
        expected_from: Option<Span>,
    },
    /// A binary operator applied to types it isn't defined for.
    ///
    /// Types are [`Message`]s: a type that isn't known yet is described in words, like "an integer".
    InvalidOperands {
        op: String,
        lhs: Message,
        rhs: Message,
        span: Span,
        /// An operand that's optional: as written, a fallback value for it, and where it is.
        optional: Option<(String, String, Span)>,
    },
    WrongArgCount {
        name: String,
        expected: usize,
        /// The function is variadic, so `expected` is a minimum.
        at_least: bool,
        found: usize,
        span: Span,
    },
    UnknownField {
        ty: String,
        field: String,
        available: Vec<String>,
        span: Span,
    },
    MissingField {
        ty: String,
        field: String,
        span: Span,
    },
    DuplicateField {
        field: String,
        span: Span,
    },
    UnknownMethod {
        ty: Message,
        method: String,
        span: Span,
    },
    /// `.field` or `.method()` on a value that isn't a struct.
    NotAStruct {
        ty: Message,
        span: Span,
    },
    NotAFunction {
        name: String,
        ty: Message,
        span: Span,
    },
    CannotDeref {
        ty: Message,
        span: Span,
    },
    /// Assigning to something that isn't a variable, field or dereference.
    InvalidAssignTarget {
        span: Span,
    },
    /// Assigning to a variable declared without `mut`, or to one of its fields, or `&mut` of it.
    NotMutable {
        name: String,
        span: Span,
        declared_at: Span,
        /// `&mut` of it, rather than an assignment.
        borrow: bool,
    },
    /// Assigning, or `&mut`, through a `&` reference, which only allows reading.
    BehindSharedReference {
        span: Span,
        borrow: bool,
    },
    /// The type of an expression isn't known where it needs to be.
    CannotInfer {
        span: Span,
    },
    /// `break;` with no `loop` or `while` around it.
    BreakOutsideLoop {
        span: Span,
    },
    /// `Sqlite3 {}` for an `extern struct`, which only C can create.
    ConstructHandle {
        name: String,
        span: Span,
    },
    /// Copying a value whose type moves: `a.get(i)` of an array of structs, or `[value; N]`.
    NotCopyable {
        ty: Message,
        span: Span,
        /// From `[value; N]`, rather than `get`.
        repeat: bool,
    },
    /// `if flag` with a `bool?`: it's unclear whether that asks for a value or for `true`.
    OptionalBoolCondition {
        span: Span,
    },
    /// `else` or `== none` on a value that isn't an optional.
    NotOptional {
        ty: Message,
        span: Span,
    },
    /// Something other than `out name` passed to an `out` parameter.
    NeedsOut {
        param: String,
        span: Span,
        declared_at: Span,
    },
}

impl TypeError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            TypeError::Mismatch {
                expected,
                found,
                span,
                expected_from,
            } => {
                let mut d = Diagnostic::error(msg![
                    "expected ",
                    expected.clone(),
                    ", found ",
                    found.clone()
                ])
                .with_label(Label::primary(
                    span.clone(),
                    msg!["this is ", found.clone()],
                ));
                if let Some(from) = expected_from {
                    d = d.with_label(Label::secondary(
                        from.clone(),
                        msg![expected.clone(), " expected because of this"],
                    ));
                }
                d
            }
            TypeError::InvalidOperands {
                op,
                lhs,
                rhs,
                span,
                optional,
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
                    None => d.with_label(Label::primary(span.clone(), "no such operation")),
                }
            }
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
                span,
            } => {
                let mut d =
                    Diagnostic::error(msg![struct_name(owner), " has no field ", field(name)])
                        .with_label(Label::primary(span.clone(), "unknown field"));
                if let Some(close) = closest_match(name, available, 2) {
                    d = d.with_hint(msg!["did you mean ", field(close), "?"]);
                } else if !available.is_empty() {
                    let mut hint = msg!["available fields: "];
                    for (i, f) in available.iter().enumerate() {
                        if i > 0 {
                            hint.0.push(", ".into());
                        }
                        hint.0.push(field(f));
                    }
                    d = d.with_hint(hint);
                }
                d
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
            } => Diagnostic::error(msg![owner.clone(), " has no method ", func(method)])
                .with_label(Label::primary(span.clone(), "unknown method")),
            TypeError::NotAStruct { ty: found, span } => {
                Diagnostic::error(msg![found.clone(), " is not a struct"]).with_label(
                    Label::primary(span.clone(), "only structs have fields and methods"),
                )
            }
            TypeError::NotAFunction {
                name,
                ty: found,
                span,
            } => Diagnostic::error(msg![var(name), " is not a function"]).with_label(
                Label::primary(span.clone(), msg!["this is ", found.clone()]),
            ),
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
            TypeError::CannotDeref { ty: found, span } => {
                Diagnostic::error(msg!["cannot dereference ", found.clone()]).with_label(
                    Label::primary(span.clone(), "only references can be dereferenced"),
                )
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
                Diagnostic::error(msg![var(name), " is immutable"])
                    .with_label(Label::primary(span.clone(), label))
                    .with_label(Label::secondary(
                        declared_at.clone(),
                        msg!["declared without ", code("mut")],
                    ))
                    .with_fix(Fix::replace(
                        msg!["make ", var(name), " mutable"],
                        declared_at.clone(),
                        format!("mut {name}"),
                    ))
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
            TypeError::InvalidAssignTarget { span } => {
                Diagnostic::error("cannot assign to this expression").with_label(Label::primary(
                    span.clone(),
                    "expected a variable, field or dereference",
                ))
            }
            TypeError::CannotInfer { span } => {
                Diagnostic::error("cannot infer the type of this expression")
                    .with_label(Label::primary(span.clone(), "add a type annotation"))
            }
            TypeError::BreakOutsideLoop { span } => {
                Diagnostic::error(msg![code("break"), " outside of a loop"]).with_label(
                    Label::primary(
                        span.clone(),
                        msg![
                            "only a ",
                            code("loop"),
                            " or ",
                            code("while"),
                            " can be left"
                        ],
                    ),
                )
            }
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
        }
    }
}

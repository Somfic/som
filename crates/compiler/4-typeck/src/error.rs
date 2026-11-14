use som_base::diagnostics::closest_match;
use som_base::message::{Message, code, field, func, struct_name, var};
use som_base::{Diagnostic, Fix, Label, Span, msg};

/// What kind of value a statement throws away.
#[derive(Debug, Clone)]
pub enum Unused {
    /// `x == 5;`, with the statement as an assignment: `x = 5`.
    Comparison(String),
    /// Anything else that only computes a value.
    Value,
}

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
        /// The trait the operands' type could implement to have the operator: `Add` for `+`;
        /// with the type parameter's name if it's one, which can require it instead.
        implement: Option<(&'static str, Option<String>)>,
        /// Writing the implementation, or the bound.
        fix: Option<Fix>,
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
    /// `break;` or `continue;` with no loop around it.
    BreakOutsideLoop {
        keyword: &'static str,
        span: Span,
    },
    /// `Sqlite3 {}` for an `extern struct`, which only C can create.
    ConstructHandle {
        name: String,
        span: Span,
    },
    /// `&str`: a `str` is already a view of the text, like a slice.
    StrBehindReference {
        span: Span,
    },
    /// A `str` passed to C, which needs text ending in a zero byte: a `cstr`.
    StrToC {
        span: Span,
    },
    /// `for x in v` where `v` isn't a range, array or slice.
    NotIterable {
        ty: Message,
        span: Span,
        /// It's text: its bytes can be.
        text: bool,
    },
    /// A function that returns something, but some path reaches its end without a value.
    MissingReturn {
        ty: Message,
        span: Span,
        returns: Option<Span>,
    },
    /// `Pair<i32, i32>` for a struct with one type parameter, or `Pair` for one with any.
    WrongTypeArgCount {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    /// A value where a type is needed: `x: N` for a value parameter `N`, or `Pair<5>`.
    ValueAsType {
        what: Message,
        span: Span,
    },
    /// A type where a value is needed: `Buffer<i32>` for `struct Buffer<N: usize>`, or
    /// `[u8; T]`. `ty` is the value's type, if one is wanted; an array's length is any integer.
    TypeAsValue {
        what: Message,
        ty: Option<Message>,
        span: Span,
    },
    /// A variant made or matched in a form it isn't: `Shape::Circle` for `Circle(f32)`, or
    /// `Shape::Rect(1.0)` for `Rect { w: f32 }`. `form` is how it's written.
    WrongVariantForm {
        name: String,
        holds: &'static str,
        form: String,
        span: Span,
    },
    /// `Shape { … }` for an enum: values are made from its variants.
    ConstructEnum {
        name: String,
        example: Option<String>,
        span: Span,
    },
    /// `t.field` on an enum where not every variant has `field`, or of the same type.
    /// `without` are the variants that don't have it; if none, they have it typed differently.
    NotInEveryVariant {
        ty: String,
        field: String,
        without: Vec<String>,
        span: Span,
    },
    /// A method taking `self` by value, called on a reference to a value that can't be copied.
    ReceiverBehindReference {
        ty: Message,
        span: Span,
    },
    /// A call where none of the functions with that name fit the arguments.
    NoMatchingOverload {
        name: String,
        args: Vec<Message>,
        /// Each function with the name, and where it's defined.
        candidates: Vec<(String, Span)>,
        span: Span,
    },
    /// A call where several functions with that name fit equally well.
    AmbiguousCall {
        name: String,
        /// Each function that fits, and where it's defined.
        candidates: Vec<(String, Span)>,
        span: Span,
    },
    /// Two functions with the same name, both taking and giving the same types.
    DuplicateOverload {
        name: String,
        span: Span,
        previous: Span,
    },
    /// An `impl Trait for …` without one of the trait's methods or associated types.
    MissingTraitItem {
        /// Each item missing, as it's declared: `fn eq(self: &Hello, other: &Hello) -> bool`.
        missing: Vec<String>,
        trait_: String,
        span: Span,
        /// Where to write them, and their text.
        fix: (Span, String),
    },
    /// An `impl Trait for …` with a method or associated type the trait doesn't have.
    NotInTrait {
        what: &'static str,
        name: String,
        trait_: String,
        span: Span,
    },
    /// A trait's method implemented with other types than the trait says.
    ImplSignatureMismatch {
        name: String,
        expected: String,
        span: Span,
        declared_at: Span,
    },
    /// A type used where a trait is required, that doesn't implement it.
    DoesNotImplement {
        ty: Message,
        trait_: String,
        span: Span,
    },
    /// A pattern names a variant the enum doesn't have.
    UnknownVariant {
        ty: String,
        variant: String,
        available: Vec<String>,
        span: Span,
    },
    /// A pattern that can't match values of the type matched: `Circle(r)` against an `i32`.
    PatternMismatch {
        pattern: String,
        ty: Message,
        span: Span,
    },
    /// `Circle(a, b)` for a variant holding one value.
    PatternFieldCount {
        variant: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    /// `Rect { w }` leaving out fields, without `..`.
    MissingFieldPatterns {
        variant: String,
        fields: Vec<String>,
        span: Span,
    },
    /// A `match` with values no arm matches. `missing` are patterns for them; `fix` adds arms
    /// for those.
    NonExhaustiveMatch {
        ty: Message,
        missing: Vec<String>,
        span: Span,
        fix: Option<Fix>,
    },
    /// An arm whose pattern only matches what earlier arms already do.
    UnreachableArm {
        span: Span,
        earlier: Option<Span>,
    },
    /// A value parameter of a type it can't be: `struct S<X: str>`.
    InvalidValueParamType {
        ty: Message,
        span: Span,
    },
    /// `return;` in a function that returns something.
    ReturnWithoutValue {
        ty: Message,
        span: Span,
    },
    /// An integer literal too big for its type: `let x: u8 = 300;`.
    LiteralOutOfRange {
        value: i64,
        ty: String,
        max: u128,
        min: i128,
        span: Span,
        /// It's a type argument, `Small<300>`, which `as` can't convert.
        type_arg: bool,
    },
    /// `impl Copy for …` of a struct with a field that's moved, not copied.
    CopyWithMovedField { ty: Message, field: String, field_ty: Message, span: Span },
    /// `x.len` of an array, slice or text, which has a method called that, not a field.
    FieldIsMethod { ty: Message, name: String, span: Span },
    /// `impl Drop for …` of a type whose values are copied.
    DropOnCopy { ty: Message, span: Span },
    /// `impl Add for i32`: a type implementing a trait whose operators it has built in.
    BuiltinImpl { ty: Message, trait_: String, span: Span },
    /// A statement whose value is thrown away, when working it out does nothing else.
    UnusedValue { kind: Unused, span: Span },
    /// `-x` of something that isn't a signed integer or a float.
    CannotNegate { ty: Message, span: Span },
    /// `value as T` where they aren't both numbers.
    InvalidCast {
        from: Message,
        to: Message,
        span: Span,
    },
    /// `a..` or `..b` somewhere other than `slice`, where nothing says where it ends.
    OpenRange {
        span: Span,
    },
    /// `*s` of a slice: what it views has no fixed size, so it can't be a value of its own.
    DerefSlice {
        span: Span,
    },
    /// `set` or `get_mut` on a `str`: text can't be changed in place.
    ReadOnlyStr {
        span: Span,
    },
    /// `set` or `get_mut` on a `[T]`, which can only read.
    ReadOnlySlice {
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
                implement,
                fix,
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
                    // Values of a type that could say how: `impl Add for …`.
                    None if let Some((trait_, param)) = implement => {
                        let mut d =
                            d.with_label(Label::primary(span.clone(), "no such operation"));
                        if let Some(fix) = fix {
                            d = d.with_fix(fix.clone());
                        }
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
            TypeError::StrBehindReference { span } => {
                Diagnostic::error(msg![code("&str"), " is a reference to a reference"])
                    .with_label(Label::primary(
                        span.clone(),
                        "a `str` already refers to its text",
                    ))
                    .with_fix(Fix::replace(
                        msg!["write ", code("str")],
                        span.clone(),
                        "str",
                    ))
            }
            TypeError::StrToC { span } => Diagnostic::error(msg!["C can't take a ", code("str")])
                .with_label(Label::primary(
                    span.clone(),
                    "C needs text ending in a zero byte",
                ))
                .with_hint(msg![
                    "pass a ",
                    code("cstr"),
                    "; a string literal becomes one"
                ]),
            TypeError::MissingReturn { ty, span, returns } => {
                let d = Diagnostic::error(msg!["not every path returns ", ty.clone()]).with_label(
                    Label::primary(span.clone(), "this end can be reached without a value"),
                );
                let d = match returns {
                    Some(at) => {
                        d.with_label(Label::secondary(at.clone(), "declared to return this"))
                    }
                    None => d,
                };
                d.with_hint(msg![
                    "end every path in ",
                    code("return"),
                    ", or end the function with a value"
                ])
            }
            TypeError::WrongTypeArgCount {
                name,
                expected,
                found,
                span,
            } => {
                let plural = |n: &usize| if *n == 1 { "" } else { "s" };
                Diagnostic::error(msg![
                    struct_name(name),
                    format!(
                        " takes {expected} type argument{}, but {found} ",
                        plural(expected)
                    ),
                    if *found == 1 {
                        "was given"
                    } else {
                        "were given"
                    }
                ])
                .with_label(Label::primary(span.clone(), "here"))
            }
            TypeError::ValueAsType { what, span } => {
                Diagnostic::error(msg![what.clone(), " is a value, not a type"])
                    .with_label(Label::primary(span.clone(), "a type is needed here"))
            }
            TypeError::TypeAsValue { what, ty, span } => {
                let wanted = match ty {
                    Some(ty) => msg!["a value of type ", ty.clone(), " is needed here"],
                    None => msg!["a number is needed here"],
                };
                Diagnostic::error(msg![what.clone(), " is a type, not a value"])
                    .with_label(Label::primary(span.clone(), wanted))
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
                        let mut m = Message(Vec::new());
                        for (i, v) in many.iter().enumerate() {
                            if i > 0 {
                                m.0.push(if i + 1 == many.len() { " and " } else { ", " }.into());
                            }
                            m.0.push(code(v));
                        }
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
                    let rest: Vec<String> =
                        missing.iter().skip(1).map(|m| format!("`{m}`")).collect();
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
            TypeError::PatternMismatch { pattern, ty, span } => {
                Diagnostic::error(msg![code(pattern), " can't match a ", ty.clone()]).with_label(
                    Label::primary(span.clone(), msg!["a ", ty.clone(), " is matched here"]),
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
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        message
                            .0
                            .push(if i + 1 == fields.len() { " and " } else { ", " }.into());
                    }
                    message.0.push(field(f));
                }
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
                let mut message = Message(Vec::new());
                for (i, m) in missing.iter().enumerate() {
                    if i > 0 {
                        message.0.push(
                            if i + 1 == missing.len() {
                                " and "
                            } else {
                                ", "
                            }
                            .into(),
                        );
                    }
                    message.0.push(code(m));
                }
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
                let mut d = Diagnostic::error(message).with_label(Label::primary(
                    span.clone(),
                    msg!["this ", ty.clone(), " can be ", what],
                ));
                if let Some(fix) = fix {
                    d = d.with_fix(fix.clone());
                }
                d
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
            TypeError::InvalidValueParamType { ty, span } => {
                Diagnostic::error(msg!["a value parameter can't be of type ", ty.clone()])
                    .with_label(Label::primary(
                        span.clone(),
                        msg!["not an integer or ", code("bool")],
                    ))
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
            TypeError::CopyWithMovedField {
                ty,
                field,
                field_ty,
                span,
            } => Diagnostic::error(msg![ty.clone(), " can't implement ", code("Copy")])
                .with_label(Label::primary(
                    span.clone(),
                    msg![code(field.clone()), " is a ", field_ty.clone(), ", which is moved, not copied"],
                )),
            TypeError::FieldIsMethod { ty, name, span } => {
                Diagnostic::error(msg![ty.clone(), " has no field ", code(name.clone())])
                    .with_label(Label::primary(span.clone(), "it's a method"))
                    .with_fix(Fix {
                        title: msg!["call it: ", code(format!("{name}()"))],
                        edits: vec![(span.after(), "()".to_string())],
                    })
            }
            TypeError::DropOnCopy { ty, span } => Diagnostic::error(msg![
                ty.clone(),
                " can't implement ",
                code("Drop")
            ])
            .with_label(Label::primary(
                span.clone(),
                "its values are copied, so each copy would be dropped",
            )),
            TypeError::BuiltinImpl { ty, trait_, span } => Diagnostic::error(msg![
                ty.clone(),
                " can't implement ",
                code(trait_)
            ])
            .with_label(Label::primary(span.clone(), "its operators are built in")),
            TypeError::UnusedValue { kind, span } => match kind {
                Unused::Comparison(assignment) => {
                    Diagnostic::error("the comparison's result is unused")
                        .with_label(Label::primary(span.clone(), "this compares, it doesn't assign"))
                        .with_fix(Fix::replace(
                            msg!["assign instead: ", code(assignment.clone())],
                            span.clone(),
                            assignment.clone(),
                        ))
                }
                Unused::Value => Diagnostic::error("this value is unused")
                    .with_label(Label::primary(span.clone(), "working it out does nothing else"))
                    .with_hint(msg![
                        "use it, or if dropping it is the point, ",
                        code("drop(…)"),
                        " it"
                    ]),
            },
            TypeError::CannotNegate { ty, span } => {
                Diagnostic::error(msg!["cannot negate ", ty.clone()]).with_label(Label::primary(
                    span.clone(),
                    "only signed integers and floats have a `-`",
                ))
            }
            TypeError::InvalidCast { from, to, span } => {
                Diagnostic::error(msg!["cannot convert ", from.clone(), " to ", to.clone()])
                    .with_label(Label::primary(
                        span.clone(),
                        "only numbers convert with `as`",
                    ))
            }
            TypeError::NotIterable { ty, span, text } => {
                let d = Diagnostic::error(msg!["cannot loop over ", ty.clone()]).with_label(
                    Label::primary(
                        span.clone(),
                        msg![
                            code("for"),
                            " goes through ranges, arrays, slices and what implements ",
                            code("Iterable"),
                            " or ",
                            code("Iterator")
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
            TypeError::ReadOnlyStr { span } => {
                Diagnostic::error(msg!["cannot change the bytes of a ", code("str")])
                    .with_label(Label::primary(span.clone(), "text can only be read"))
            }
            TypeError::ReadOnlySlice { span } => {
                Diagnostic::error(msg!["cannot change the elements of a ", code("[T]")])
                    .with_label(Label::primary(span.clone(), "this slice can only read"))
                    .with_hint(msg!["changing them needs a ", code("[mut T]")])
            }
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

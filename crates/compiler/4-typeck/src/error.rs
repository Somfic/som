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

/// What a value C can't take is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotForC {
    Optional,
    Reference,
    Other,
}

#[derive(Debug, Clone)]
pub enum TypeError {
    /// A field, method or property used where it can't be seen: private to its type, or only
    /// visible in its module.
    NotVisible {
        name: String,
        span: Span,
        hidden: som_resolve::visibility::Hidden,
    },
    /// `Hello { … }` where some of `Hello`'s fields can't be seen: only its own functions can
    /// make one, which `fixes` call.
    ConstructHidden {
        ty: String,
        /// The fields it can't see, and where each is defined.
        fields: Vec<(String, Span)>,
        span: Span,
        fixes: Vec<Fix>,
    },
    /// `found` was used where `expected` is required. `expected_from` points at what required it.
    /// `f(key)` where `f` takes a reference, `&K`: the value has to be borrowed. `expected` is
    /// the reference type written out, `&K`, even where the call hadn't worked it out yet.
    NotBorrowed {
        expected: Message,
        found: Message,
        /// It's a field, `entry.value`: the field itself, reached through `entry`, not a
        /// reference to it.
        field: bool,
        span: Span,
        expected_from: Option<Span>,
        /// `&key` or `&mut key`.
        fix: Fix,
    },
    Mismatch {
        expected: Message,
        found: Message,
        span: Span,
        expected_from: Option<Span>,
        /// A `&T` where a `T` is expected: dereferencing it.
        fix: Option<Fix>,
        /// An optional where something else is expected: the code giving it, and whether
        /// that's a variable, for hints on checking it first.
        optional: Option<(String, bool)>,
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
        /// What the left operand's type does take on the right, through its implementations:
        /// `usize` for `"ab" * 1.5`.
        takes: Vec<String>,
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
        /// The type's properties, read the same way: `v.magnitude`.
        props: Vec<String>,
        /// Its methods, called: `v.normalized()`.
        methods: Vec<String>,
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
        /// What the type has that's seen from here: its methods, properties and fields.
        methods: Vec<String>,
        props: Vec<String>,
        fields: Vec<String>,
        /// The call with its `()`, when it's given nothing: what a property goes in place of.
        bare_call: Option<Span>,
    },
    /// `.field` or `.method()` on a value that isn't a struct.
    NotAStruct {
        ty: Message,
        span: Span,
    },
    /// `entry.value` of an `entry` that's a reference to a reference, `&mut &Pair`: a field is
    /// reached through one reference, so the extra one is likely a mistake.
    ReferenceToReference {
        object: String,
        ty: Message,
        span: Span,
    },
    /// `get(i).value`: a field or method of an optional, which may hold nothing. `object` is
    /// what's written for it; `variable`, whether that's a variable `if` can check as it is.
    MemberOfOptional {
        object: String,
        ty: Message,
        variable: bool,
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
    /// `[i32; W]` where `W` isn't a whole number that's not below 0.
    ConstNotCount {
        name: String,
        span: Span,
        defined_at: Span,
    },
    /// `[i32; N]` of `@const N = four();`: `N` is worked out by running code, which happens
    /// after types are checked.
    /// `@for i in 0..n`, `@if ready`: what's settled while compiling has to be known then.
    NotSettled {
        what: &'static str,
        hint: &'static str,
        span: Span,
    },
    /// `v2.z` where `z` is in `@if N >= 3 { … }` among `Vector`'s members, and `v2` is a
    /// `Vector<2, f32>`.
    MemberNotHere {
        name: String,
        ty: Message,
        condition: String,
        span: Span,
    },
    /// `v.z` where `v` is a `Vector<N, f32>` of a generic `N`: `z` is only there where
    /// `N >= 3`, which isn't known here.
    MemberMaybeNotHere {
        name: String,
        condition: String,
        span: Span,
    },
    /// `@for f in T` where `T` isn't a struct: a generic function's type parameter, used with
    /// one that isn't (`i32`), or a name that's neither.
    ReflectNotStruct { name: String, span: Span },
    /// `@for (i, k) in T` among a `match`'s arms where `T` isn't an enum.
    ReflectNotEnum { name: String, span: Span },
    /// A type where a value goes in an `@if`'s condition, not compared with another type:
    /// `@if T`, or `@if T == 3`.
    TypeNotCompared { span: Span },
    /// `@on fail fn save()`: a listener takes a `&Failure`, and gives nothing back.
    ListenerSignature { span: Span },
    /// `@const N = len();` where what `len` runs needs `N`'s value to be checked: a type
    /// `[i32; N]`, or an `@if`. It can't be worked out before it's known.
    ConstCycle { name: String, span: Span },
    /// `none` where a raw pointer is expected: `fflush(none)`. A null pointer is
    /// `mem::null()`.
    NoneAsPointer {
        ty: Message,
        span: Span,
        expected_from: Option<Span>,
    },
    /// `W = 5` or `&mut W` of `@const W = 60;`.
    ConstChanged {
        name: String,
        span: Span,
        defined_at: Span,
        borrow: bool,
    },
    /// Assigning to something that isn't a variable, field or dereference.
    InvalidAssignTarget {
        span: Span,
    },
    /// `c.n = 5` where `n` is a property: it's computed, not stored. `field` is the struct's
    /// name and its field of the same name, private from here, that the property reads.
    AssignToProp {
        name: String,
        span: Span,
        field: Option<(String, som_resolve::visibility::Hidden)>,
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
    /// `fn main(args: …)`: the entry takes nothing; arguments come from `args()`.
    /// `main` giving what isn't an exit code: a script ending in `"done"`.
    MainReturns {
        ty: Message,
        span: Span,
        /// Its return type is written, rather than its last value deciding it.
        written: bool,
    },
    MainParams {
        span: Span,
    },
    /// Changing what a `#T` shares, `node.value = 5`: every copy sees it.
    SharedChanged {
        span: Span,
        borrow: bool,
    },
    /// `str` or `&str`, text before it was `string`.
    StrIsString {
        span: Span,
    },
    /// A `string` passed to C, which takes a `cstr`: its `c_str`.
    StrToC {
        span: Span,
    },
    /// An optional, reference or struct among a C function's extra arguments, `printf`'s:
    /// C would read its bytes as a number or pointer.
    NotForC {
        ty: Message,
        what: NotForC,
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
    /// `Pair<i32, i32, i32>` for `struct Pair<A, B = A>`: between `required` and `most` type
    /// arguments, the rest having defaults.
    WrongTypeArgRange {
        name: String,
        required: usize,
        most: usize,
        found: usize,
        span: Span,
    },
    /// `type A = B; type B = A;`: an alias that stands for itself, so it has no type to stand for.
    AliasCycle {
        name: String,
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
    /// Functions sharing a name that differ only in taking `&T` or `&mut T`: `view(&self)`
    /// and `view(&mut self)`. `span` is the `&mut` one's name.
    MutOverload {
        name: String,
        span: Span,
        previous: Span,
        /// Naming it `…_mut`.
        fix: Fix,
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
    /// Rust's `Some(v)` or `None`, matching an optional: som matches the value it holds with
    /// the pattern itself, `v` (after a `none` arm, for a name), and writes `none`.
    RustOptionalPattern {
        written: String,
        span: Span,
        fix: Option<Fix>,
        /// It's `Some(name)` before any `none` arm, where `name` would match the optional.
        needs_none_first: bool,
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
        value: i128,
        ty: String,
        max: u128,
        min: i128,
        span: Span,
        /// It's a type argument, `Small<300>`, which `as` can't convert.
        type_arg: bool,
    },
    /// `impl Copy for …` of a struct with a field that's moved, not copied.
    CopyWithMovedField {
        ty: Message,
        field: String,
        field_ty: Message,
        span: Span,
    },
    /// `x.len` of an array, slice or text, which has a method called that, not a field.
    FieldIsMethod {
        ty: Message,
        name: String,
        span: Span,
    },
    /// `items[0]` of something that isn't an array: a slice's length isn't known, so only
    /// `get` can say whether it has the element.
    IndexNotArray {
        ty: Message,
        span: Span,
        get: String,
    },
    /// `{ begin(); end() }`: a call giving nothing at the end of a block, without a `;`.
    UnitTail {
        span: Span,
    },
    /// `"{x}"` of a value whose type doesn't implement `Display`.
    NotDisplay {
        ty: Message,
        span: Span,
    },
    /// `values[i] = v` of what doesn't implement `IndexMut`: a `string`.
    SliceIndexWrite {
        ty: Message,
        span: Span,
        /// What to write instead, which says whether it found it: `values.set(i, …)`.
        instead: String,
    },
    /// `values[3]` of a `[T; 3]`.
    IndexOutOfBounds {
        index: u128,
        len: u128,
        span: Span,
    },
    /// `values[0]` of a `[T; N]` whose `N` isn't known here, so might be 0.
    IndexUnknownLength {
        len: String,
        span: Span,
        get: String,
    },
    /// `prop reset { … }`: a property that gives nothing back only does something.
    PropReturnsNothing {
        name: String,
        span: Span,
    },
    /// `v.x` of a `Vector<i32, 3>`, where `x` is in `impl<T> Vector<T, 1>`: a method or
    /// property only some of the struct's types have.
    NotOnImplTarget {
        ty: Message,
        /// `"method"` or `"property"`.
        what: &'static str,
        name: String,
        /// Each impl block's target that has one, as written in terms of its parameters
        /// (`Vector<T, 1>`), and where.
        targets: Vec<(String, Span)>,
        span: Span,
    },
    /// `list.len()` of a property, which is read without parentheses: `list.len`. `parens`
    /// is where the `()` is, when there's nothing in it.
    MethodIsProp {
        ty: Message,
        name: String,
        span: Span,
        parens: Option<Span>,
    },
    /// `*item` of a `&T`, in generic code where `T` may be a type that moves: taking it would
    /// leave the reference's owner with a value someone else now owns. `fix` requires `T: Copy`.
    MoveOutOfReference {
        place: String,
        ty: Message,
        span: Span,
        fix: Option<Fix>,
    },
    /// `impl Drop for …` of a type whose values are copied.
    DropOnCopy {
        ty: Message,
        span: Span,
    },
    /// `impl Share for …` of a type whose values are copied bit for bit.
    ShareOnCopy {
        ty: Message,
        span: Span,
    },
    /// `impl Add for i32`: a type implementing a trait whose operators it has built in.
    BuiltinImpl {
        ty: Message,
        trait_: String,
        span: Span,
    },
    /// A statement whose value is thrown away, when working it out does nothing else.
    UnusedValue {
        kind: Unused,
        span: Span,
    },
    /// `-x` of something that isn't a signed integer or a float.
    CannotNegate {
        ty: Message,
        span: Span,
    },
    /// `~x` of something other than an integer.
    CannotFlipBits {
        ty: Message,
        span: Span,
    },
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
                for (i, (name, _)) in fields.iter().enumerate() {
                    if i > 0 {
                        hint.0
                            .push(if i + 1 == fields.len() { " and " } else { ", " }.into());
                    }
                    hint.0.push(field(name.clone()));
                }
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
                let mut d = Diagnostic::error(msg![
                    "expected ",
                    expected.clone(),
                    ", found ",
                    found.clone()
                ])
                .with_label(Label::primary(span.clone(), label));
                if let Some(from) = expected_from {
                    d = d.with_label(Label::secondary(
                        from.clone(),
                        msg![expected.clone(), " expected because of this"],
                    ));
                }
                d.with_fix(fix.clone())
            }
            TypeError::Mismatch {
                expected,
                found,
                span,
                expected_from,
                fix,
                optional,
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
                if let Some(fix) = fix {
                    d = d.with_fix(fix.clone());
                }
                if let Some((object, variable)) = optional {
                    d = optional_hints(d, object, *variable);
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
                    // Values of a type that could say how: `impl Add for …`.
                    None if let Some((trait_, param)) = implement => {
                        let mut d = d.with_label(Label::primary(span.clone(), "no such operation"));
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
                    // `"ab" * 3`: it has the operator, with something else on the right.
                    None if !takes.is_empty() => {
                        let mut hint = msg![lhs.clone(), " has ", code(op), " with "];
                        for (i, ty) in takes.iter().enumerate() {
                            if i > 0 {
                                hint.push(if i + 1 == takes.len() { " or " } else { ", " });
                            }
                            hint.push(code(ty));
                        }
                        hint.push(" on the right");
                        let d = d
                            .with_label(Label::primary(span.clone(), "no such operation"))
                            .with_hint(hint);
                        // `"score: " + n`: written into the text instead.
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
            TypeError::MainReturns { ty, span, written } => {
                let d = Diagnostic::error(msg![code("main"), " can't give ", ty.clone()])
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
                Diagnostic::error(msg![code("main"), " takes no parameters"])
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
            TypeError::StrIsString { span } => {
                Diagnostic::error(msg!["som's text is ", code("string")])
                    .with_label(Label::primary(
                        span.clone(),
                        msg!["text is ", code("string")],
                    ))
                    .with_fix(Fix::replace(
                        msg!["change to ", code("string")],
                        span.clone(),
                        "string",
                    ))
            }
            TypeError::NotForC { ty, what, span } => {
                let d = Diagnostic::error(msg!["C can't take ", ty.clone()]).with_label(
                    Label::primary(
                        span.clone(),
                        "C reads it as the number or pointer its format says",
                    ),
                );
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
                let d = Diagnostic::error(msg!["C can't take a ", code("string")])
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
            TypeError::WrongTypeArgRange {
                name,
                required,
                most,
                found,
                span,
            } => Diagnostic::error(msg![
                struct_name(name),
                format!(" takes {required} to {most} type arguments, but {found} "),
                if *found == 1 {
                    "was given"
                } else {
                    "were given"
                }
            ])
            .with_label(Label::primary(span.clone(), "here"))
            .with_hint(msg![match most - required {
                1 => "the last one has a default, and can be left out".to_string(),
                n => format!("the last {n} have defaults, and can be left out"),
            }]),
            TypeError::AliasCycle { name, span } => {
                Diagnostic::error(msg!["type alias ", struct_name(name), " refers to itself"])
                    .with_label(Label::primary(span.clone(), "used in its own definition"))
                    .with_hint(msg![
                        "an alias stands for its type everywhere, so it can't contain itself"
                    ])
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
            TypeError::RustOptionalPattern {
                written,
                span,
                fix,
                needs_none_first,
            } => {
                let (label, hint) = match written.as_str() {
                    "None" => (
                        msg!["an optional without a value is ", code("none")],
                        msg![
                            "a capitalised name here would be a new variable, matching anything"
                        ],
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
            } => Diagnostic::error(msg![ty.clone(), " can't implement ", code("Copy")]).with_label(
                Label::primary(
                    span.clone(),
                    msg![
                        code(field.clone()),
                        " is a ",
                        field_ty.clone(),
                        ", which is moved, not copied"
                    ],
                ),
            ),
            TypeError::FieldIsMethod { ty, name, span } => {
                Diagnostic::error(msg![ty.clone(), " has no field ", code(name.clone())])
                    .with_label(Label::primary(span.clone(), "it's a method"))
                    .with_fix(Fix {
                        title: msg!["call it: ", code(format!("{name}()"))],
                        edits: vec![(span.after(), "()".to_string())],
                    })
            }
            TypeError::IndexNotArray { ty, span, get } => {
                Diagnostic::error(msg!["cannot index ", ty.clone(), " with ", code("[]")])
                    .with_label(Label::primary(
                        span.clone(),
                        msg!["not an array, and it doesn't implement ", code("Index")],
                    ))
                    .with_hint(msg!["use ", code(get.clone()), ", which gives an optional"])
            }
            TypeError::UnitTail { span } => Diagnostic::error("expected `;`")
                .with_label(Label::primary(
                    span.after(),
                    "expected a semicolon after the call",
                ))
                .with_fix(Fix::replace(msg!["add the ", code(";")], span.after(), ";")),
            TypeError::NotDisplay { ty, span } => {
                Diagnostic::error(msg![ty.clone(), " can't be written into text"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg!["it doesn't implement ", code("Display")],
                    ))
                    .with_hint(msg![
                        "implement ",
                        code("Display"),
                        " for it: ",
                        code("prop display_len"),
                        " says how many bytes its text takes, and ",
                        code("display"),
                        " writes them"
                    ])
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
            TypeError::NotOnImplTarget {
                ty,
                what,
                name,
                targets,
                span,
            } => {
                // `Vector<T, 2>`, `Vector<T, 2> and Vector<T, 3>`, or a list ending in `and`.
                let mut only = msg![code(name.clone()), " is only defined for "];
                for (i, (target, _)) in targets.iter().enumerate() {
                    if i > 0 {
                        only.push(if i + 1 == targets.len() {
                            " and "
                        } else {
                            ", "
                        });
                    }
                    only.push(code(target.clone()));
                }
                let mut diagnostic =
                    Diagnostic::error(msg![ty.clone(), " has no ", *what, " ", func(name.clone())])
                        .with_label(Label::primary(span.clone(), only));
                for (_, at) in targets {
                    diagnostic =
                        diagnostic.with_label(Label::secondary(at.clone(), "defined for this"));
                }
                diagnostic
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
                    code("Copy")
                ]);
                match fix {
                    Some(fix) => d.with_fix(fix.clone()),
                    None => d,
                }
            }
            TypeError::DropOnCopy { ty, span } => {
                Diagnostic::error(msg![ty.clone(), " can't implement ", code("Drop")]).with_label(
                    Label::primary(
                        span.clone(),
                        "its values are copied, so each copy would be dropped",
                    ),
                )
            }
            TypeError::ShareOnCopy { ty, span } => {
                Diagnostic::error(msg![ty.clone(), " can't implement ", code("Share")]).with_label(
                    Label::primary(
                        span.clone(),
                        msg![
                            "it's ",
                            code("Copy"),
                            ": its copies are made bit for bit, so ",
                            code("share"),
                            " would never be called"
                        ],
                    ),
                )
            }
            TypeError::BuiltinImpl { ty, trait_, span } => {
                Diagnostic::error(msg![ty.clone(), " can't implement ", code(trait_)])
                    .with_label(Label::primary(span.clone(), "its operators are built in"))
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
                    .with_hint(msg!["changing them needs a ", code("&mut [T]")])
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
                if name == "self" && !typed {
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
                    .with_hint(
                        "a property is worked out when it's read; there's nothing to assign to",
                    ),
            },
            TypeError::NoneAsPointer {
                ty,
                span,
                expected_from,
            } => {
                let mut d = Diagnostic::error(msg![code("none"), " isn't a pointer"])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![ty.clone(), " expected, which ", code("none"), " can't be"],
                    ));
                if let Some(from) = expected_from {
                    d = d.with_label(Label::secondary(
                        from.clone(),
                        msg![ty.clone(), " expected because of this"],
                    ));
                }
                d.with_hint(msg![
                    code("none"),
                    " is an optional without a value; a null pointer is ",
                    code("som::mem::null()")
                ])
                .with_fix(Fix::replace(
                    msg!["change to ", code("som::mem::null()")],
                    span.clone(),
                    "som::mem::null()",
                ))
            }
            TypeError::ConstNotCount {
                name,
                span,
                defined_at,
            } => Diagnostic::error(msg!["constant ", var(name), " isn't a count"])
                .with_label(Label::primary(
                    span.clone(),
                    "a whole number, at least 0, is needed here",
                ))
                .with_label(Label::secondary(defined_at.clone(), "defined here")),
            TypeError::NotSettled { what, hint, span } => {
                Diagnostic::error(msg![*what, " has to be known while compiling"])
                    .with_label(Label::primary(span.clone(), "only known when the program runs"))
                    .with_hint(*hint)
            }
            TypeError::MemberNotHere {
                name,
                ty,
                condition,
                span,
            } => Diagnostic::error(msg![ty.clone(), " has no ", code(name)])
                .with_label(Label::primary(
                    span.clone(),
                    msg![code(name), " is only there where ", code(condition)],
                )),
            TypeError::MemberMaybeNotHere {
                name,
                condition,
                span,
            } => Diagnostic::error(msg!["can't tell ", code(name), " is there"])
                .with_label(Label::primary(
                    span.clone(),
                    msg!["only there where ", code(condition), ", which isn't known here"],
                ))
                .with_hint(msg![
                    "use it in ",
                    code(format!("@if {condition} {{ … }}")),
                    ", which is checked for each instance"
                ]),
            TypeError::TypeNotCompared { span } => {
                Diagnostic::error("a type can only be compared with another type")
                    .with_label(Label::primary(span.clone(), "a type, where a value goes"))
                    .with_hint(msg![
                        "an ",
                        code("@if"),
                        " can ask which type it is: ",
                        code("@if T == f32 { … }")
                    ])
            }
            TypeError::ReflectNotEnum { name, span } => {
                Diagnostic::error(msg![code(name), " isn't an enum to go through the variants of"])
                    .with_label(Label::primary(span.clone(), "not an enum here"))
                    .with_hint(msg![
                        code("@for (i, k) in T"),
                        " among a match's arms goes through an enum's variants: an enum, or a \
                         type parameter used with one"
                    ])
            }
            TypeError::ReflectNotStruct { name, span } => {
                Diagnostic::error(msg![code(name), " isn't a struct to go through the fields of"])
                    .with_label(Label::primary(span.clone(), "not a struct here"))
                    .with_hint(msg![
                        code("@for f in T"),
                        " goes through a struct's fields: a struct, or a type parameter used \
                         with one"
                    ])
            }
            TypeError::ListenerSignature { span } => Diagnostic::error(msg![
                "an ",
                code("@on fail"),
                " function takes what failed"
            ])
            .with_label(Label::primary(span.clone(), "called when the program fails"))
            .with_hint(msg!["write it ", code("fn name(failure: &Failure)")]),
            TypeError::ConstCycle { name, span } => {
                Diagnostic::error(msg!["constant ", var(name), " is needed to work itself out"])
                    .with_label(Label::primary(
                        span.clone(),
                        "what it runs needs its value, in a type or an `@if`",
                    ))
                    .with_hint("its value can't be known before it's worked out")
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

/// How to get at what optional `object` holds: check it first (inside `if object { … }` if
/// it's a `variable`, else after putting it in one), or give a fallback for `none`.
fn optional_hints(d: Diagnostic, object: &str, variable: bool) -> Diagnostic {
    let check = if variable {
        msg![
            "check it first: inside ",
            code(format!("if {object} {{ … }}")),
            ", it holds one"
        ]
    } else {
        msg![
            "check it first: put it in a variable and use it inside ",
            code("if value { … }")
        ]
    };
    d.with_hint(check).with_hint(msg![
        "or give a fallback for ",
        code("none"),
        ": ",
        code(format!("{object} else …"))
    ])
}

/// What `name`, a member the type doesn't have, could be instead: how som names another
/// language's member (`size` → `len`), else the closest one it has, else all it has. `span`
/// is the name; `bare_call` the call with its `()` when it's given nothing; `called`
/// whether it was called, `x.name()`, or read, `x.name`.
#[allow(clippy::too_many_arguments)]
fn member_hints(
    d: Diagnostic,
    name: &str,
    span: &Span,
    bare_call: Option<&Span>,
    called: bool,
    fields: &[String],
    props: &[String],
    methods: &[String],
) -> Diagnostic {
    let read = |n: &str| fields.iter().chain(props).any(|f| f == n);
    let is_method = |n: &str| methods.iter().any(|m| m == n);
    let foreign = som_base::foreign::member(name).filter(|f| read(f.som) || is_method(f.som));
    let all: Vec<String> = (fields.iter().chain(props).chain(methods))
        .cloned()
        .collect();
    let target = match foreign {
        Some(found) => Some(found.som),
        None => closest_match(name, &all, 2),
    };
    if let Some(target) = target {
        let hint = match foreign {
            Some(found) => msg!["som calls it ", code(target), ": ", found.hint()],
            None => msg!["did you mean ", code(target), "?"],
        };
        // Where it goes: a property read in place of a call that's given nothing,
        // `xs.size()` → `xs.len`; otherwise just the name.
        let replaced = match (read(target), called) {
            (true, true) => bare_call.cloned(),
            (true, false) => Some(span.clone()),
            (false, true) => Some(span.clone()),
            (false, false) => None,
        };
        let d = d.with_hint(hint);
        return match replaced {
            Some(at) => d.with_fix(Fix::replace(
                msg!["change to ", code(target)],
                at,
                target.to_string(),
            )),
            None => d,
        };
    }
    if all.is_empty() {
        return d;
    }
    let mut hint = Message(Vec::new());
    for (label, names) in [
        ("methods", methods),
        ("properties", props),
        ("fields", fields),
    ] {
        if names.is_empty() {
            continue;
        }
        if !hint.0.is_empty() {
            hint.0.push("; ".into());
        }
        hint.0.push(format!("{label}: ").into());
        for (i, n) in names.iter().enumerate() {
            if i > 0 {
                hint.0.push(", ".into());
            }
            hint.0.push(code(n));
        }
    }
    d.with_hint(msg!["it has ", hint])
}

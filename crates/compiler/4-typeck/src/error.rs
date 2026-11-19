mod calls;
mod consts;
mod mismatch;
mod patterns;
mod places;
mod traits;
mod types;
mod visibility;

use som_base::diagnostics::closest_match;
use som_base::message::{Message, code};
use som_base::{Diagnostic, Fix, Label, Span, msg};

#[derive(Debug, Clone)]
pub enum Unused {
    /// `x == 5;`, with the statement as an assignment: `x = 5`.
    Comparison(String),
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotForC {
    Optional,
    Reference,
    Other,
}

#[derive(Debug, Clone)]
pub enum TypeError {
    NotVisible {
        name: String,
        span: Span,
        hidden: som_resolve::visibility::Hidden,
    },
    /// Only `Hello`'s own functions can make one; `fixes` call them.
    ConstructHidden {
        ty: String,
        fields: Vec<(String, Span)>,
        span: Span,
        fixes: Vec<Fix>,
    },
    /// `expected` is the reference type written out, `&K`, even where the call hadn't worked
    /// it out yet.
    NotBorrowed {
        expected: Message,
        found: Message,
        /// It's a field, `entry.value`: the field itself, reached through `entry`, not a
        /// reference to it.
        field: bool,
        span: Span,
        expected_from: Option<Span>,
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
    /// Types are `Message`s: one not known yet is described in words, like "an integer".
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
        props: Vec<String>,
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
        /// What the type has that's visible from here.
        methods: Vec<String>,
        props: Vec<String>,
        fields: Vec<String>,
        /// The call with its `()`, when it's given nothing: what a property goes in place of.
        bare_call: Option<Span>,
    },
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
    ConstNotCount {
        name: String,
        span: Span,
        defined_at: Span,
    },
    /// What's settled while compiling (`@for i in 0..n`, `@if ready`) has to be known then.
    NotSettled {
        what: &'static str,
        hint: &'static str,
        span: Span,
    },
    MemberNotHere {
        name: String,
        ty: Message,
        condition: String,
        span: Span,
    },
    /// `v.z` of a `Vector<N, f32>` with generic `N`: `z` is only there where `N >= 3`, which
    /// isn't known here.
    MemberMaybeNotHere {
        name: String,
        condition: String,
        span: Span,
    },
    ReflectNotStruct {
        name: String,
        span: Span,
    },
    ReflectNotEnum {
        name: String,
        span: Span,
    },
    /// A type in an `@if` condition, not compared with another type: `@if T`, `@if T == 3`.
    TypeNotCompared {
        span: Span,
    },
    /// A listener takes a `&Failure` and gives nothing back.
    ListenerSignature {
        span: Span,
    },
    /// A `@test` takes nothing, gives nothing back, and isn't generic.
    TestSignature {
        span: Span,
    },
    /// `@const N = len();` where checking what `len` runs needs `N`'s value: it can't be worked
    /// out before it's known.
    ConstCycle {
        name: String,
        span: Span,
    },
    NoneAsPointer {
        ty: Message,
        span: Span,
        expected_from: Option<Span>,
    },
    ConstChanged {
        name: String,
        span: Span,
        defined_at: Span,
        borrow: bool,
    },
    InvalidAssignTarget {
        span: Span,
    },
    /// `field` is the struct's name and its private field of the same name that the property
    /// reads.
    AssignToProp {
        name: String,
        span: Span,
        field: Option<(String, som_resolve::visibility::Hidden)>,
    },
    NotMutable {
        name: String,
        span: Span,
        declared_at: Span,
        /// `&mut` of it, rather than an assignment.
        borrow: bool,
    },
    /// `total = total + n` inside `n => { … }`: a closure only reads what it captures.
    CaptureChanged {
        name: String,
        span: Span,
        /// Moved out, rather than changed.
        moved: bool,
    },
    BehindSharedReference {
        span: Span,
        borrow: bool,
    },
    CannotInfer {
        span: Span,
    },
    BreakOutsideLoop {
        keyword: &'static str,
        span: Span,
    },
    /// `Sqlite3 {}` for an `extern struct`, which only C can create.
    ConstructHandle {
        name: String,
        span: Span,
    },
    MainReturns {
        ty: Message,
        span: Span,
        /// Its return type is written, rather than its last value deciding it.
        written: bool,
    },
    /// The entry takes nothing; arguments come from `args()`.
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
    NotIterable {
        ty: Message,
        span: Span,
        /// It's text: its bytes can be.
        text: bool,
    },
    MissingReturn {
        ty: Message,
        span: Span,
        returns: Option<Span>,
    },
    WrongTypeArgCount {
        name: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    /// Between `required` and `most` type arguments, the rest having defaults.
    WrongTypeArgRange {
        name: String,
        required: usize,
        most: usize,
        found: usize,
        span: Span,
    },
    AliasCycle {
        name: String,
        span: Span,
    },
    ValueAsType {
        what: Message,
        span: Span,
    },
    /// `ty` is the value's type, if one is wanted; an array's length is any integer.
    TypeAsValue {
        what: Message,
        ty: Option<Message>,
        span: Span,
    },
    /// `form` is how it's written.
    WrongVariantForm {
        name: String,
        holds: &'static str,
        form: String,
        span: Span,
    },
    /// Values are made from its variants.
    ConstructEnum {
        name: String,
        example: Option<String>,
        span: Span,
    },
    /// `without` are the variants lacking it; if empty, they have it typed differently.
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
    NoMatchingOverload {
        name: String,
        args: Vec<Message>,
        candidates: Vec<(String, Span)>,
        span: Span,
    },
    AmbiguousCall {
        name: String,
        candidates: Vec<(String, Span)>,
        span: Span,
    },
    DuplicateOverload {
        name: String,
        span: Span,
        previous: Span,
    },
    /// `view(&self)` and `view(&mut self)`. `span` is the `&mut` one's name.
    MutOverload {
        name: String,
        span: Span,
        previous: Span,
        /// Naming it `…_mut`.
        fix: Fix,
    },
    MissingTraitItem {
        /// Each item missing, as it's declared: `fn eq(self: &Hello, other: &Hello) -> bool`.
        missing: Vec<String>,
        trait_: String,
        span: Span,
        /// Where to write them, and their text.
        fix: (Span, String),
    },
    NotInTrait {
        what: &'static str,
        name: String,
        trait_: String,
        span: Span,
    },
    ImplSignatureMismatch {
        name: String,
        expected: String,
        span: Span,
        declared_at: Span,
    },
    DoesNotImplement {
        ty: Message,
        trait_: String,
        span: Span,
    },
    /// `impl Index for [T]` without `impl Sized for [T]`, which `Index` builds on.
    MissingSuperImpl {
        ty: Message,
        trait_: String,
        missing: String,
        span: Span,
    },
    /// `where(is_odd)` for an `F: fn(&i32) -> bool` of what isn't called that way.
    NotCalledSo {
        ty: Message,
        signature: String,
        span: Span,
    },
    UnknownVariant {
        ty: String,
        variant: String,
        available: Vec<String>,
        span: Span,
    },
    RustOptionalPattern {
        written: String,
        span: Span,
        fix: Option<Fix>,
        /// It's `Some(name)` before any `none` arm, where `name` would match the optional.
        needs_none_first: bool,
    },
    PatternMismatch {
        pattern: String,
        ty: Message,
        span: Span,
    },
    PatternFieldCount {
        variant: String,
        expected: usize,
        found: usize,
        span: Span,
    },
    MissingFieldPatterns {
        variant: String,
        fields: Vec<String>,
        span: Span,
    },
    /// `missing` are patterns for the values no arm matches; `fix` adds arms for them.
    NonExhaustiveMatch {
        ty: Message,
        missing: Vec<String>,
        span: Span,
        fix: Option<Fix>,
    },
    UnreachableArm {
        span: Span,
        earlier: Option<Span>,
    },
    InvalidValueParamType {
        ty: Message,
        span: Span,
    },
    ReturnWithoutValue {
        ty: Message,
        span: Span,
    },
    LiteralOutOfRange {
        value: i128,
        ty: String,
        max: u128,
        min: i128,
        span: Span,
        /// It's a type argument, `Small<300>`, which `as` can't convert.
        type_arg: bool,
    },
    CopyWithMovedField {
        ty: Message,
        field: String,
        field_ty: Message,
        span: Span,
    },
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
    UnitTail {
        span: Span,
    },
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
    IndexOutOfBounds {
        index: u128,
        len: u128,
        span: Span,
    },
    /// `N` isn't known here, so it might be 0.
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
    /// A method or property only some of the struct's types have: `x` in
    /// `impl<T> Vector<T, 1>`.
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
    /// `parens` is where the `()` is, when there's nothing in it.
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
    DropOnCopy {
        ty: Message,
        span: Span,
    },
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
    CannotNegate {
        ty: Message,
        span: Span,
    },
    CannotFlipBits {
        ty: Message,
        span: Span,
    },
    InvalidCast {
        from: Message,
        to: Message,
        span: Span,
    },
    /// Outside `slice`, nothing says where it ends.
    OpenRange {
        span: Span,
    },
    /// What a slice views has no fixed size, so it can't be a value of its own.
    DerefSlice {
        span: Span,
    },
    ReadOnlyStr {
        span: Span,
    },
    ReadOnlySlice {
        span: Span,
    },
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
    NotOptional {
        ty: Message,
        span: Span,
    },
    NeedsOut {
        param: String,
        span: Span,
        declared_at: Span,
    },
}

impl TypeError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            TypeError::NotVisible { .. } | TypeError::ConstructHidden { .. } => {
                visibility::diagnostic(self)
            }
            TypeError::NotBorrowed { .. }
            | TypeError::Mismatch { .. }
            | TypeError::InvalidOperands { .. }
            | TypeError::CannotDeref { .. }
            | TypeError::NotAStruct { .. }
            | TypeError::ReferenceToReference { .. }
            | TypeError::MemberOfOptional { .. }
            | TypeError::StrIsString { .. }
            | TypeError::NotForC { .. }
            | TypeError::StrToC { .. }
            | TypeError::MissingReturn { .. }
            | TypeError::ReturnWithoutValue { .. }
            | TypeError::LiteralOutOfRange { .. }
            | TypeError::UnitTail { .. }
            | TypeError::CannotNegate { .. }
            | TypeError::CannotFlipBits { .. }
            | TypeError::InvalidCast { .. }
            | TypeError::NotIterable { .. }
            | TypeError::OpenRange { .. }
            | TypeError::DerefSlice { .. }
            | TypeError::NotCopyable { .. }
            | TypeError::OptionalBoolCondition { .. }
            | TypeError::NotOptional { .. }
            | TypeError::NoneAsPointer { .. }
            | TypeError::CannotInfer { .. }
            | TypeError::IndexNotArray { .. }
            | TypeError::IndexOutOfBounds { .. }
            | TypeError::IndexUnknownLength { .. }
            | TypeError::UnusedValue { .. }
            | TypeError::BreakOutsideLoop { .. } => mismatch::diagnostic(self),
            TypeError::NotMutable { .. }
            | TypeError::CaptureChanged { .. }
            | TypeError::BehindSharedReference { .. }
            | TypeError::AssignToProp { .. }
            | TypeError::InvalidAssignTarget { .. }
            | TypeError::SharedChanged { .. }
            | TypeError::ReadOnlyStr { .. }
            | TypeError::ReadOnlySlice { .. }
            | TypeError::SliceIndexWrite { .. }
            | TypeError::MoveOutOfReference { .. }
            | TypeError::ConstChanged { .. }
            | TypeError::ReceiverBehindReference { .. } => places::diagnostic(self),
            TypeError::WrongArgCount { .. }
            | TypeError::UnknownField { .. }
            | TypeError::MissingField { .. }
            | TypeError::DuplicateField { .. }
            | TypeError::UnknownMethod { .. }
            | TypeError::NotAFunction { .. }
            | TypeError::NoMatchingOverload { .. }
            | TypeError::AmbiguousCall { .. }
            | TypeError::DuplicateOverload { .. }
            | TypeError::MutOverload { .. }
            | TypeError::FieldIsMethod { .. }
            | TypeError::MethodIsProp { .. }
            | TypeError::PropReturnsNothing { .. }
            | TypeError::NotOnImplTarget { .. }
            | TypeError::NeedsOut { .. }
            | TypeError::MainReturns { .. }
            | TypeError::MainParams { .. }
            | TypeError::ListenerSignature { .. }
            | TypeError::TestSignature { .. }
            | TypeError::ConstructHandle { .. }
            | TypeError::NotInEveryVariant { .. } => calls::diagnostic(self),
            TypeError::WrongTypeArgCount { .. }
            | TypeError::WrongTypeArgRange { .. }
            | TypeError::AliasCycle { .. }
            | TypeError::ValueAsType { .. }
            | TypeError::TypeAsValue { .. }
            | TypeError::InvalidValueParamType { .. } => types::diagnostic(self),
            TypeError::MissingTraitItem { .. }
            | TypeError::NotInTrait { .. }
            | TypeError::ImplSignatureMismatch { .. }
            | TypeError::DoesNotImplement { .. }
            | TypeError::NotCalledSo { .. }
            | TypeError::MissingSuperImpl { .. }
            | TypeError::CopyWithMovedField { .. }
            | TypeError::NotDisplay { .. }
            | TypeError::DropOnCopy { .. }
            | TypeError::ShareOnCopy { .. }
            | TypeError::BuiltinImpl { .. } => traits::diagnostic(self),
            TypeError::UnknownVariant { .. }
            | TypeError::RustOptionalPattern { .. }
            | TypeError::PatternMismatch { .. }
            | TypeError::PatternFieldCount { .. }
            | TypeError::MissingFieldPatterns { .. }
            | TypeError::NonExhaustiveMatch { .. }
            | TypeError::UnreachableArm { .. }
            | TypeError::WrongVariantForm { .. }
            | TypeError::ConstructEnum { .. } => patterns::diagnostic(self),
            TypeError::ConstNotCount { .. }
            | TypeError::NotSettled { .. }
            | TypeError::ReflectNotStruct { .. }
            | TypeError::ReflectNotEnum { .. }
            | TypeError::TypeNotCompared { .. }
            | TypeError::ConstCycle { .. }
            | TypeError::MemberNotHere { .. }
            | TypeError::MemberMaybeNotHere { .. } => consts::diagnostic(self),
        }
    }
}

trait DiagnosticExt {
    /// Points at `from`, what made `expected` the type expected there, if that's known.
    fn expected_from(self, from: &Option<Span>, expected: &Message) -> Self;
    fn with_optional_fix(self, fix: &Option<Fix>) -> Self;
}

impl DiagnosticExt for Diagnostic {
    fn expected_from(self, from: &Option<Span>, expected: &Message) -> Self {
        match from {
            Some(from) => self.with_label(Label::secondary(
                from.clone(),
                msg![expected.clone(), " expected because of this"],
            )),
            None => self,
        }
    }

    fn with_optional_fix(self, fix: &Option<Fix>) -> Self {
        match fix {
            Some(fix) => self.with_fix(fix.clone()),
            None => self,
        }
    }
}

pub(super) fn optional_hints(d: Diagnostic, object: &str, variable: bool) -> Diagnostic {
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

/// How som names another language's member (`size` → `len`), else the closest one it has,
/// else all it has. `bare_call` is the call with its `()` when it's given nothing.
#[allow(clippy::too_many_arguments)]
pub(super) fn member_hints(
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
            Some(at) => d.with_fix(Fix::change_to(at, target.to_string())),
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

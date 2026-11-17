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
    ReflectNotStruct {
        name: String,
        span: Span,
    },
    /// `@for (i, k) in T` among a `match`'s arms where `T` isn't an enum.
    ReflectNotEnum {
        name: String,
        span: Span,
    },
    /// A type where a value goes in an `@if`'s condition, not compared with another type:
    /// `@if T`, or `@if T == 3`.
    TypeNotCompared {
        span: Span,
    },
    /// `@on fail fn save()`: a listener takes a `&Failure`, and gives nothing back.
    ListenerSignature {
        span: Span,
    },
    /// `@const N = len();` where what `len` runs needs `N`'s value to be checked: a type
    /// `[i32; N]`, or an `@if`. It can't be worked out before it's known.
    ConstCycle {
        name: String,
        span: Span,
    },
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

/// How to get at what optional `object` holds: check it first (inside `if object { … }` if
/// it's a `variable`, else after putting it in one), or give a fallback for `none`.
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

/// What `name`, a member the type doesn't have, could be instead: how som names another
/// language's member (`size` → `len`), else the closest one it has, else all it has. `span`
/// is the name; `bare_call` the call with its `()` when it's given nothing; `called`
/// whether it was called, `x.name()`, or read, `x.name`.
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

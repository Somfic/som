use som_ast::{Ast, Expr, Stmt};
use som_base::Source;
use som_resolve::Names;
use som_typeck::{IntTy, Ty, TypeError, Typed, check};
use std::sync::Arc;

fn run(source: &str) -> (Ast, Names, Typed, Vec<TypeError>) {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = check(&ast, &names);
    (ast, names, typed, errors)
}

fn ok(source: &str) -> (Ast, Names, Typed) {
    let (ast, names, typed, errors) = run(source);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    (ast, names, typed)
}

fn errors(source: &str) -> Vec<TypeError> {
    run(source).3
}

/// The type of the first `let` in the program, as written in som.
fn first_let(ast: &Ast, names: &Names, typed: &Typed) -> String {
    let (stmt, _) = ast
        .stmts
        .iter()
        .find(|(_, s)| matches!(s, Stmt::Let { .. }))
        .unwrap();
    typed.display(typed.local(names.local(stmt).unwrap()), names)
}

#[test]
fn integer_literals_default_to_i32() {
    let (ast, names, typed) = ok("fn main() { let x = 5; }");
    assert_eq!(first_let(&ast, &names, &typed), "i32");
}

#[test]
fn integer_literals_take_the_annotated_type() {
    let (ast, names, typed) = ok("fn main() { let x: u8 = 5; }");
    assert_eq!(first_let(&ast, &names, &typed), "u8");
}

#[test]
fn integer_literal_is_not_a_bool() {
    match &errors("fn main() { let x: bool = 5; }")[..] {
        [
            TypeError::Mismatch {
                expected, found, ..
            },
        ] => {
            assert_eq!(
                (expected.plain().as_str(), found.plain().as_str()),
                // A type that isn't known yet is described in plain words.
                ("`bool`", "an integer")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn return_types_are_inferred_across_functions() {
    ok("fn two() { 2 } fn main() -> i32 { two() + 1 }");
}

#[test]
fn operators_on_unknown_types_are_checked_once_known() {
    // `a + b` is seen before anything says what `a` is; the return type decides it later.
    let (_, _, typed) = ok("fn add(a, b) -> i32 { a + b }");
    let sig = typed.func(som_ast::Id::new(0));
    assert!(
        sig.params
            .iter()
            .all(|&p| *typed.ty(p) == Ty::Int(IntTy::I32))
    );
}

#[test]
fn unconstrained_parameter_is_an_error() {
    assert!(matches!(
        errors("fn f(x) {}")[..],
        [TypeError::CannotInfer { .. }]
    ));
}

#[test]
fn generic_parameters_are_opaque_inside_their_function() {
    assert!(matches!(
        errors("fn f<T>(x: T) -> i32 { x }")[..],
        [TypeError::Mismatch { .. }]
    ));
    assert!(matches!(
        errors("fn f<T>(x: T) -> T { x + x }")[..],
        [TypeError::InvalidOperands { .. }]
    ));
}

#[test]
fn generic_calls_record_their_type_arguments() {
    let (ast, names, typed) = ok("fn id<T>(x: T) -> T { x } fn run() -> bool { id(true) }");
    let (call, _) = ast
        .exprs
        .iter()
        .find(|(_, e)| matches!(e, Expr::Call { .. }))
        .unwrap();
    let args: Vec<String> = typed
        .type_args(call)
        .iter()
        .map(|&t| typed.display(t, &names))
        .collect();
    assert_eq!(args, ["bool"]);
}

#[test]
fn each_generic_call_is_instantiated_separately() {
    ok("fn id<T>(x: T) -> T { x } fn main() -> i32 { let b: bool = id(true); id(1) }");
}

#[test]
fn fields_and_methods_work_through_references() {
    let (ast, _, typed) = ok("struct P { x: i32, y: i32 }
         impl P { fn sum(self: &P) -> i32 { self.x + self.y } }
         fn f(p: &P) -> i32 { p.y + p.sum() }");
    let fields: Vec<usize> = ast
        .exprs
        .iter()
        .filter_map(|(id, _)| typed.field(id))
        .collect();
    assert_eq!(fields, [0, 1, 1]);
    let (call, _) = ast
        .exprs
        .iter()
        .find(|(_, e)| matches!(e, Expr::MethodCall { .. }))
        .unwrap();
    assert!(typed.method(call).is_some());
}

#[test]
fn method_receiver_must_match_self() {
    let errs = errors(
        "struct P { x: i32 } impl P { fn get(self: &mut P) -> i32 { self.x } } fn f(p: &P) -> i32 { p.get() }",
    );
    assert!(matches!(errs[..], [TypeError::Mismatch { .. }]), "{errs:?}");
}

#[test]
fn mutability_of_references_must_match() {
    let errs = errors("fn f() { let x = 1; let r: &mut i32 = &x; }");
    assert!(matches!(errs[..], [TypeError::Mismatch { .. }]), "{errs:?}");
}

#[test]
fn pointer_arithmetic() {
    ok("extern { fn malloc(n: i32) -> *; } fn f() -> * { malloc(8) + 4 }");
    let errs = errors("extern { fn malloc(n: i32) -> *; } fn f() -> * { malloc(8) + true }");
    assert!(
        matches!(errs[..], [TypeError::InvalidOperands { .. }]),
        "{errs:?}"
    );
}

#[test]
fn only_places_can_be_assigned() {
    assert!(matches!(
        errors("fn f() { 1 = 2; }")[..],
        [TypeError::InvalidAssignTarget { .. }]
    ));
    ok("struct P { x: i32 } fn f(p: &mut P, r: &mut i32) { p.x = 1; *r = 2; }");
}

#[test]
fn only_references_can_be_dereferenced() {
    assert!(matches!(
        errors("fn f() -> i32 { let x = 1; *x }")[..],
        [TypeError::CannotDeref { .. }]
    ));
}

#[test]
fn constructor_fields_are_checked() {
    let errs = errors("struct P { x: i32, y: i32 } fn f() { P { x: 1, x: 2, z: 3 }; }");
    let kinds: Vec<String> = errs
        .iter()
        .map(|e| format!("{e:?}").split(' ').next().unwrap().to_string())
        .collect();
    assert_eq!(kinds, ["DuplicateField", "UnknownField", "MissingField"]);
}

#[test]
fn conditional_branches_must_agree() {
    let errs = errors("fn f() -> i32 { 1 if true else false }");
    assert!(matches!(errs[..], [TypeError::Mismatch { .. }]), "{errs:?}");
}

#[test]
fn unknown_fields_offer_the_properties_of_the_value_type() {
    let errs = errors(
        "struct V<N: usize> { values: [i32; N] }
         impl V<2> { prop magnitude = 2; }
         impl V<3> { prop magnitude = 3; prop z = 0; }
         impl V<4> { prop w = 0; }
         fn f() -> i32 { let v = V { values: [1, 2, 3] }; v.manitude }",
    );
    // Each name once, and not `w`, which a `V<3>` doesn't have.
    match &errs[..] {
        [
            TypeError::UnknownField {
                field,
                available,
                props,
                ..
            },
        ] => {
            assert_eq!(field, "manitude");
            assert_eq!(available, &["values"]);
            assert_eq!(props, &["magnitude", "z"]);
        }
        _ => panic!("{errs:?}"),
    }
}

/// The edits of the `Mismatch` reported for `source`'s one error.
fn mismatch_fix(source: &str) -> Option<Vec<String>> {
    match &errors(source)[..] {
        [TypeError::Mismatch { fix, .. }] => fix
            .as_ref()
            .map(|f| f.edits.iter().map(|(_, text)| text.clone()).collect()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_reference_to_a_copy_where_the_value_is_expected_can_be_dereferenced() {
    assert_eq!(
        mismatch_fix("fn f(x: &i32) -> i32 { x } fn main() {}"),
        Some(vec!["*".to_string()])
    );
    // Anything but a name or what's read from one goes in parentheses.
    assert_eq!(
        mismatch_fix("fn f(x: &i32) -> i32 { if true { x } else { x } } fn main() {}"),
        Some(vec!["*(".to_string(), ")".to_string()])
    );
}

#[test]
fn a_reference_to_a_type_parameter_needs_copy_first() {
    // Requiring `T: Copy` needs the standard library's `Copy`, which these tests don't load:
    // without it there's no fix. The corpus has the rest.
    assert_eq!(mismatch_fix("fn f<T>(x: &T) -> T { x } fn main() {}"), None);
}

#[test]
fn a_reference_to_a_value_that_moves_is_not_dereferenced() {
    // `*x` would move it out of the reference, which never compiles.
    assert_eq!(
        mismatch_fix("struct P { v: i32 } fn f(x: &P) -> P { x } fn main() {}"),
        None
    );
}

#[test]
fn moving_a_type_parameter_out_of_a_reference_is_an_error_even_unused() {
    let errs = errors("fn f<T>(x: &T) -> T { *x } fn main() {}");
    assert!(
        matches!(errs[..], [TypeError::MoveOutOfReference { .. }]),
        "{errs:?}"
    );
    // A borrow is fine. (`T: Copy` is too: see the corpus, which has the standard library.)
    assert!(
        errors("struct P<T> { v: T } fn f<T>(p: &P<T>) -> &T { &p.v } fn main() {}").is_empty()
    );
}

/// The edits of the `NotBorrowed` reported for `source`'s one error.
fn borrow_fix(source: &str) -> Vec<String> {
    match &errors(source)[..] {
        [TypeError::NotBorrowed { fix, .. }] => {
            fix.edits.iter().map(|(_, text)| text.clone()).collect()
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_value_where_a_reference_is_expected_can_be_borrowed() {
    assert_eq!(
        borrow_fix("fn f(x: &i32) -> i32 { *x } fn main() { let n: i32 = 1; f(n); }"),
        ["&"]
    );
    assert_eq!(
        borrow_fix("fn f(x: &mut i32) {} fn main() { let mut n: i32 = 1; f(n); }"),
        ["&mut "]
    );
    // Anything but a name or what's read from one goes in parentheses.
    assert_eq!(
        borrow_fix("fn f(x: &i32) -> i32 { *x } fn main() { let n: i32 = 1; f(n + 1); }"),
        ["&(", ")"]
    );
}

#[test]
fn a_field_where_an_optional_reference_is_expected_can_be_borrowed() {
    // `&i32?` takes a reference too: the field is borrowed as it is.
    assert_eq!(
        borrow_fix("struct P { v: i32 } fn f(p: &P) -> &i32? { p.v } fn main() {}"),
        ["&"]
    );
}

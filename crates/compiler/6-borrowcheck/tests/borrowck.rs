use som_base::Source;
use som_borrowcheck::BorrowError;
use std::sync::Arc;

fn check(source: &str) -> Vec<BorrowError> {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    let mir = som_lower::lower(&ast, &names, &typed);
    som_borrowcheck::check(&mir)
}

fn kinds(source: &str) -> Vec<String> {
    check(source)
        .iter()
        .map(|e| format!("{e:?}").split(' ').next().unwrap().to_string())
        .collect()
}

const BIG: &str = "struct Big { a: i32 } fn take(b: Big) -> i32 { b.a } ";

#[test]
fn loans_flow_through_function_results() {
    // `first` returns a reference derived from its argument, so `r` keeps `x` borrowed.
    let src = "fn first(a: &mut i32) -> &mut i32 { a } fn f() { let mut x = 1; let r = first(&mut x); let y = x; }";
    assert_eq!(kinds(src), ["UseWhileMutBorrowed"]);
}

#[test]
fn loans_flow_through_struct_fields() {
    let src =
        "struct R { r: &mut i32 } fn f() { let mut x = 1; let h = R { r: &mut x }; let y = x; }";
    assert_eq!(kinds(src), ["UseWhileMutBorrowed"]);
}

#[test]
fn borrows_passed_to_a_call_end_with_it() {
    let src = "fn bump(a: &mut i32) { *a = *a + 1; } fn f() -> i32 { let mut x = 1; bump(&mut x); bump(&mut x); x }";
    assert!(check(src).is_empty());
}

#[test]
fn unused_results_release_their_loans() {
    let src = "fn same(a: &i32) -> &i32 { a } fn f() { let mut x = 1; same(&x); let r = &mut x; }";
    assert!(check(src).is_empty());
}

#[test]
fn writing_through_a_mutable_reference_is_fine() {
    assert!(check("fn f() { let mut x = 1; let r = &mut x; *r = 2; *r = *r + 1; }").is_empty());
}

#[test]
fn reborrows_keep_the_original_loan_alive() {
    let src = "fn f() -> i32 { let mut x = 1; let r = &mut x; let s = &mut *r; x }";
    assert_eq!(kinds(src), ["UseWhileMutBorrowed"]);
}

#[test]
fn assigning_a_borrowed_value_is_an_error() {
    assert_eq!(
        kinds("fn f() { let mut x = 1; let r = &x; x = 2; }"),
        ["AssignWhileBorrowed"]
    );
}

#[test]
fn moving_a_borrowed_value_is_an_error() {
    let src = format!("{BIG} fn f() {{ let b = Big {{ a: 1 }}; let r = &b; take(b); }}");
    assert_eq!(kinds(&src), ["MoveWhileBorrowed"]);
}

#[test]
fn moving_out_of_a_reference_is_an_error() {
    let src = format!("{BIG} fn f(r: &Big) -> i32 {{ take(*r) }}");
    assert_eq!(kinds(&src), ["MoveOutOfReference"]);
}

#[test]
fn copy_fields_can_be_read_after_others_are_moved() {
    let src = "struct In { v: i32 } struct Out { inner: In, n: i32 } fn take(i: In) -> i32 { i.v }
               fn f(o: Out) -> i32 { take(o.inner) + o.n }";
    assert!(check(src).is_empty());
}

#[test]
fn a_partially_moved_struct_cannot_be_used_whole() {
    let src = "struct In { v: i32 } struct Out { inner: In } fn take(i: In) -> i32 { i.v } fn use_out(o: Out) {}
               fn f(o: Out) { take(o.inner); use_out(o); }";
    assert_eq!(kinds(src), ["UseAfterMove"]);
}

#[test]
fn a_loop_can_borrow_again_each_iteration() {
    let src = "fn f() { let mut x = 1; let mut i = 0; while i < 3 { let r = &mut x; *r = i; i = i + 1; } }";
    assert!(check(src).is_empty());
}

#[test]
fn a_borrow_kept_across_iterations_conflicts() {
    let src = "struct H { r: &mut i32 } fn f() { let mut x = 1; let mut y = 2; let mut h = H { r: &mut y }; let mut i = 0;
               while i < 3 { let r = &mut x; i = i + 1; } let z = &mut y; }";
    assert_eq!(kinds(src), ["ConflictingBorrow"]);
}

#[test]
fn diagnostics_name_the_place() {
    let errors = check("struct P { x: i32 } fn f(p: P) { let r = &mut p.x; let y = p.x; }");
    match &errors[..] {
        [BorrowError::UseWhileMutBorrowed { name, .. }] => assert_eq!(name, "p.x"),
        other => panic!("{other:?}"),
    }
}

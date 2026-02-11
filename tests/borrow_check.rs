use som::borrow_check::BorrowError;
use som::{BorrowChecker, Source, TypeInferencer, parser};
use std::sync::Arc;

fn check(source: &str) -> Vec<BorrowError> {
    let source = Arc::new(Source::from_raw(source));
    let (ast, _) = parser::parse(source);
    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    let mut checker = BorrowChecker::new(&typed_ast);
    checker.check_program()
}

fn has_error<F: Fn(&BorrowError) -> bool>(errors: &[BorrowError], predicate: F) -> bool {
    errors.iter().any(predicate)
}

#[test]
fn test_use_after_move() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let y = x;
            let z = x;
        }
        "#,
    );
    // i32 is Copy, so no error expected
    assert!(errors.is_empty());
}

#[test]
fn test_conflicting_borrow_mut_then_immut() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let s = &x;
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_conflicting_borrow_immut_then_mut() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let s = &mut x;
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_multiple_immutable_borrows_ok() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let s = &x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_dangling_reference_direct() {
    let errors = check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            &x
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_dangling_reference_through_variable() {
    let errors = check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            let r = &x;
            r
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_parameter_reference_ok() {
    let errors = check(
        r#"
        fn test(x: &i32) -> &i32 {
            x
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_borrow_parameter_ok() {
    let errors = check(
        r#"
        fn test(x: i32) -> &i32 {
            &x
        }
        "#,
    );
    // x is a parameter at scope 0, so &x should be ok to return
    assert!(errors.is_empty());
}

#[test]
fn test_borrow_expires_after_scope() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &mut x;
            }
            let s = &mut x;
        }
        "#,
    );
    // First borrow expires when inner block ends, second borrow is ok
    assert!(errors.is_empty());
}

#[test]
fn test_use_while_mut_borrowed() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let y = x + 1;
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_nested_block_dangling() {
    let errors = check(
        r#"
        fn test() -> &i32 {
            let r = {
                let x = 10;
                &x
            };
            r
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_outer_scope_reference_ok() {
    let errors = check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            let r = {
                &x
            };
            r
        }
        "#,
    );
    // x is in outer scope, so returning &x from inner block is ok
    // But returning from function is still dangling!
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_double_mut_borrow() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let s = &mut x;
        }
        "#,
    );
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_reborrow_after_scope() {
    let errors = check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &x;
                let s = &x;
            }
            let t = &mut x;
        }
        "#,
    );
    // Immutable borrows expire, mutable borrow is ok
    assert!(errors.is_empty());
}

#[test]
fn test_static_lifetime_ok() {
    let errors = check(
        r#"
        fn test(x: &'static i32) -> &'static i32 {
            x
        }
        "#,
    );
    // 'static references never dangle
    assert!(errors.is_empty());
}

#[test]
fn test_string_literal_no_dangling() {
    let errors = check(
        r#"
        fn test() -> &str {
            "hello"
        }
        "#,
    );
    // String literals are &'static str, never dangle
    assert!(errors.is_empty());
}

#[test]
fn test_string_literal_in_block() {
    let errors = check(
        r#"
        fn test() -> &str {
            let s = {
                "world"
            };
            s
        }
        "#,
    );
    // String literals propagate their 'static origin through blocks
    assert!(errors.is_empty());
}

#[test]
fn test_string_literal_assigned_to_variable() {
    let errors = check(
        r#"
        fn test() -> &str {
            let s = "hello";
            s
        }
        "#,
    );
    // String literals propagate 'static through variable bindings
    assert!(errors.is_empty());
}

#[test]
fn test_bool_literals() {
    let errors = check(
        r#"
        fn test() {
            let t = true;
            let f = false;
        }
        "#,
    );
    // Bool literals are Copy, no borrow issues
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_no_borrow_issues() {
    let errors = check(
        r#"
        fn test(b: bool) -> i32 {
            let x = 10;
            x if b else 20
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_borrow_in_branch() {
    let errors = check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let r = &x if b else &x;
        }
        "#,
    );
    // Both branches borrow x immutably - should be fine
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_dangling_reference() {
    let errors = check(
        r#"
        fn test(b: bool) -> &i32 {
            let x = 10;
            &x if b else &x
        }
        "#,
    );
    // Returns reference to local - should be dangling
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_conditional_copy_in_branch() {
    let errors = check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let y = x if b else 20;
            let z = x;
        }
        "#,
    );
    // i32 is Copy, so this should be fine
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_with_mut_borrow_no_conflict() {
    let errors = check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let r = &mut x;
            let y = 1 if b else 2;
        }
        "#,
    );
    // No conflict - y doesn't use x
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_use_while_borrowed() {
    let errors = check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let r = &mut x;
            let y = x if b else 2;
        }
        "#,
    );
    // x is used while mutably borrowed
    assert!(has_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_conditional_borrow_expires_before_use() {
    let errors = check(
        r#"
        fn test(b: bool) -> i32 {
            let x = 10;
            {
                let r = &mut x;
            };
            x if b else 20
        }
        "#,
    );
    // Borrow expires, so using x is fine
    assert!(errors.is_empty());
}

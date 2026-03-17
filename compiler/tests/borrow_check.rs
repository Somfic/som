mod common;

use common::{has_borrow_error, test_borrow_check};
use som::borrow_check::BorrowError;

#[test]
fn test_use_after_move() {
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let s = &x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_conflicting_borrow_immut_then_mut() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let s = &mut x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_multiple_immutable_borrows_ok() {
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            &x
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_dangling_reference_through_variable() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            let r = &x;
            r
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_parameter_reference_ok() {
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &mut x;
            };
            let s = &mut x;
        }
        "#,
    );
    // First borrow expires when inner block ends, second borrow is ok
    assert!(errors.is_empty());
}

#[test]
fn test_use_while_mut_borrowed() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let y = x + 1;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_nested_block_dangling() {
    let errors = test_borrow_check(
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
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_outer_scope_reference_ok() {
    let errors = test_borrow_check(
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
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_double_mut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let s = &mut x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_reborrow_after_scope() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &x;
                let s = &x;
            };
            let t = &mut x;
        }
        "#,
    );
    // Immutable borrows expire, mutable borrow is ok
    assert!(errors.is_empty());
}

#[test]
fn test_static_lifetime_ok() {
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) -> &i32 {
            let x = 10;
            &x if b else &x
        }
        "#,
    );
    // Returns reference to local - should be dangling
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_conditional_copy_in_branch() {
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
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
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let r = &mut x;
            let y = x if b else 2;
        }
        "#,
    );
    // x is used while mutably borrowed
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_conditional_borrow_expires_before_use() {
    let errors = test_borrow_check(
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

// === While loop borrow patterns (should pass) ===

#[test]
fn test_while_with_immutable_borrow_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let mut i = 0;
            while i < 3 {
                i = i + 1;
            }
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_while_body_local_borrows_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let mut i = 0;
            while i < 3 {
                let y = 5;
                let r = &y;
                i = i + 1;
            }
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_while_no_borrow_conflict() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let mut x = 0;
            while x < 10 {
                x = x + 1;
            }
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Function parameter borrow patterns (should pass) ===

#[test]
fn test_two_param_refs_ok() {
    let errors = test_borrow_check(
        r#"
        fn test(a: &i32, b: &i32) -> &i32 {
            a
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_param_ref_not_dangling() {
    let errors = test_borrow_check(
        r#"
        fn test(x: &i32) -> &i32 {
            let y = 10;
            x
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Multiple variables (should pass) ===

#[test]
fn test_separate_borrows_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 1;
            let y = 2;
            let rx = &x;
            let ry = &y;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_separate_mut_borrows_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 1;
            let y = 2;
            let rx = &mut x;
            let ry = &mut y;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_multiple_immutable_three_refs() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let a = &x;
            let b = &x;
            let c = &x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Sequential borrow/release (should pass) ===

#[test]
fn test_sequential_mut_borrows_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &mut x;
            };
            {
                let s = &mut x;
            };
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_mut_then_immut_after_scope() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &mut x;
            };
            let s = &x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_immut_then_mut_after_scope() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &x;
            };
            let s = &mut x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_three_sequential_borrows() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let a = &x;
            };
            {
                let b = &mut x;
            };
            {
                let c = &x;
            };
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Deeply nested scopes (should pass) ===

#[test]
fn test_deeply_nested_borrow_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                {
                    {
                        let r = &x;
                    }
                };
            };
            let s = &mut x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_nested_blocks_separate_borrows() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let y = 20;
            {
                let rx = &x;
                {
                    let ry = &mut y;
                }
            };
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Struct value patterns (should pass) ===

#[test]
fn test_struct_field_is_copy() {
    let errors = test_borrow_check(
        r#"
        struct P { x: i32 }
        fn test() {
            let p = P { x: 5 };
            let x = p.x;
            let y = p.x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === String literal patterns (should pass) ===

#[test]
fn test_string_literal_multiple() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &str {
            let a = "hello";
            let b = "world";
            a
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_string_literal_in_conditional() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) -> &str {
            "yes" if b else "no"
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_string_literal_assigned_twice() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &str {
            let s = "first";
            let s = "second";
            s
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Conditional borrow patterns (should pass) ===

#[test]
fn test_conditional_with_copies_ok() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) {
            let x = 10;
            let y = 20;
            let z = x if b else y;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_both_static() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) -> &str {
            let a = "yes";
            let b2 = "no";
            a if b else b2
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Error cases ===

#[test]
fn test_error_mut_borrow_then_use() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let y = x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_error_mut_borrow_then_immut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 5;
            let r = &mut x;
            let s = &x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_error_immut_borrow_then_mut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 7;
            let r = &x;
            let s = &mut x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_error_double_mut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 3;
            let a = &mut x;
            let b = &mut x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_error_dangling_from_block() {
    let errors = test_borrow_check(
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
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_error_dangling_simple() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &i32 {
            let x = 5;
            &x
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_error_dangling_through_let() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &i32 {
            let x = 5;
            let r = &x;
            r
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

#[test]
fn test_error_use_in_addition_while_borrowed() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let y = x + 5;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_error_mut_borrow_then_read_in_conditional() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) {
            let x = 42;
            let r = &mut x;
            let y = x if b else 99;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_error_dangling_mut_ref() {
    let errors = test_borrow_check(
        r#"
        fn test() -> &mut i32 {
            let x = 10;
            &mut x
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

// === More pass cases ===

#[test]
fn test_copy_i32_after_borrow_expires() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &mut x;
            };
            let y = x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_multiple_functions_separate_scopes() {
    let errors = test_borrow_check(
        r#"
        fn foo() {
            let x = 10;
            let r = &x;
        }
        fn bar() {
            let y = 20;
            let s = &mut y;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_bool_copy_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let b = true;
            let c = b;
            let d = b;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_nested_scope_borrow_released() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &x;
                let s = &x;
            };
            let t = &mut x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_param_copy_ok() {
    let errors = test_borrow_check(
        r#"
        fn test(x: i32) {
            let a = x;
            let b = x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_block_returns_copy() {
    let errors = test_borrow_check(
        r#"
        fn test() -> i32 {
            let x = {
                let y = 5;
                y
            };
            x
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_no_borrow_with_literals() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) -> i32 {
            1 if b else 2
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_while_with_copy_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() -> i32 {
            let x = 10;
            let mut sum = 0;
            let mut i = 0;
            while i < x {
                sum = sum + 1;
                i = i + 1;
            }
            sum
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Additional while loop patterns ===

#[test]
fn test_while_condition_uses_borrow_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let mut i = 0;
            while i < 5 {
                i = i + 1;
            }
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_while_with_multiple_locals() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let mut i = 0;
            while i < 3 {
                let a = 1;
                let b = 2;
                let c = a + b;
                i = i + 1;
            }
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Additional conditional patterns ===

#[test]
fn test_conditional_nested_copies() {
    let errors = test_borrow_check(
        r#"
        fn test(a: bool, b: bool) -> i32 {
            let x = 1;
            let y = 2;
            let z = 3;
            x if a else (y if b else z)
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_conditional_ref_to_param() {
    let errors = test_borrow_check(
        r#"
        fn test(x: &i32, y: &i32, b: bool) -> &i32 {
            x if b else y
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_error_conditional_dangling_one_branch() {
    let errors = test_borrow_check(
        r#"
        fn test(x: &i32, b: bool) -> &i32 {
            let y = 10;
            x if b else &y
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::DanglingReference { .. }
    )));
}

// === More scope and borrow lifetime tests ===

#[test]
fn test_borrow_in_inner_scope_no_conflict() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let r = &x;
            };
            {
                let s = &x;
            };
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_error_conflicting_borrow_across_let() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 100;
            let r = &mut x;
            let y = 20;
            let s = &x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

#[test]
fn test_two_variables_independent_mut_borrows() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let a = 1;
            let b = 2;
            let ra = &mut a;
            let rb = &mut b;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_immutable_borrow_does_not_block_read() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &x;
            let y = x;
        }
        "#,
    );
    // i32 is Copy, reading x while immutably borrowed is fine
    assert!(errors.is_empty());
}

#[test]
fn test_error_use_after_mut_borrow_in_expr() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let r = &mut x;
            let y = x + x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

// === Function with multiple params and borrows ===

#[test]
fn test_param_and_local_borrows_ok() {
    let errors = test_borrow_check(
        r#"
        fn test(x: &i32) -> &i32 {
            let y = 10;
            let r = &y;
            x
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_return_param_ref_with_local_mut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test(x: &i32) -> &i32 {
            let y = 10;
            let r = &mut y;
            x
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Complex scope nesting ===

#[test]
fn test_four_levels_deep_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                {
                    {
                        {
                            let r = &x;
                        }
                    }
                };
            };
            let s = &mut x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_sequential_scopes_with_different_borrows() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
                let a = &x;
            };
            {
                let b = &mut x;
            };
            {
                let c = &mut x;
            };
            {
                let d = &x;
            };
        }
        "#,
    );
    assert!(errors.is_empty());
}

// === Arithmetic and expressions with borrows ===

#[test]
fn test_arithmetic_no_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() -> i32 {
            let x = 10;
            let y = 20;
            x + y
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_complex_arithmetic_copies() {
    let errors = test_borrow_check(
        r#"
        fn test() -> i32 {
            let a = 1;
            let b = 2;
            let c = 3;
            let d = a + b + c;
            let e = a + d;
            e
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_error_triple_mut_borrow() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            let a = &mut x;
            let b = &mut x;
            let c = &mut x;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::ConflictingBorrow { .. }
    )));
}

// === Edge cases ===

#[test]
fn test_empty_block_ok() {
    let errors = test_borrow_check(
        r#"
        fn test() {
            let x = 10;
            {
            };
            let r = &mut x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_borrow_of_parameter_copy() {
    let errors = test_borrow_check(
        r#"
        fn test(x: i32) {
            let r = &x;
            let s = &x;
        }
        "#,
    );
    assert!(errors.is_empty());
}

#[test]
fn test_error_mut_borrow_of_param_then_use() {
    let errors = test_borrow_check(
        r#"
        fn test(x: i32) {
            let r = &mut x;
            let y = x + 1;
        }
        "#,
    );
    assert!(has_borrow_error(&errors, |e| matches!(
        e,
        BorrowError::UseWhileMutBorrowed { .. }
    )));
}

#[test]
fn test_string_literal_from_conditional_in_block() {
    let errors = test_borrow_check(
        r#"
        fn test(b: bool) -> &str {
            let s = {
                "hello" if b else "world"
            };
            s
        }
        "#,
    );
    assert!(errors.is_empty());
}

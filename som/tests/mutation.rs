mod common;
use common::*;
use som::source_raw;

// ============================================================================
// Basic assignment
// ============================================================================

#[test]
fn assign_variable() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        x = 10;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn assign_multiple() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 1;
        x = 2;
        x = 3;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

// ============================================================================
// Mutable variables
// ============================================================================

#[test]
fn let_mut_basic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 5;
        x = 10;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn let_mut_in_loop() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut i = 0;
        while i < 10 {
            i = i + 1;
        }
        i
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn let_mut_multiple_vars() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 1;
        let mut b = 2;
        a = a + 10;
        b = b + 20;
        a + b
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 33); // (1+10) + (2+20) = 11 + 22 = 33
}

// ============================================================================
// Mutable references
// ============================================================================

#[test]
fn mut_ref_read() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 42;
        let r = &mut x;
        *r
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
#[ignore = "writing through mutable ref then reading original causes borrow error"]
fn mut_ref_write() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        let r = &mut x;
        *r = 10;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
#[ignore = "writing through mutable ref then reading original causes borrow error"]
fn mut_ref_write_and_read() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 5;
        let r = &mut x;
        *r = *r + 10;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
#[ignore = "writing through mutable ref then reading original causes borrow error"]
fn mut_ref_in_function() {
    let source = source_raw!(
        r#"
    fn increment(r: &mut i32) {
        *r = *r + 1;
    }

    fn main() -> i32 {
        let mut x = 10;
        increment(&mut x);
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 11);
}

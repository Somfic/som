mod common;
use common::*;
use som::source_raw;

// ============================================================================
// Loop statements
// ============================================================================

#[test]
#[ignore = "loop keyword not fully implemented (parsing issue)"]
fn loop_with_return() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        loop {
            return 5;
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn while_basic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        while x < 10 {
            x = x + 1;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn while_false() {
    // Loop body should never execute
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 42;
        while false {
            x = 0;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn nested_while() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 0;
        while i < 3 {
            let mut j = 0;
            while j < 3 {
                sum = sum + 1;
                j = j + 1;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 9); // 3 * 3 = 9
}

// ============================================================================
// If/else statements
// ============================================================================

#[test]
fn if_statement_basic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        if true {
            x = 42;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn if_else_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        if false {
            x = 1;
        } else {
            x = 2;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn if_no_else() {
    // If condition is false and no else branch, variable should be unchanged
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 100;
        if false {
            x = 0;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn if_else_chain() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 2;
        let mut result = 0;
        if x == 1 {
            result = 10;
        } else if x == 2 {
            result = 20;
        } else {
            result = 30;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20);
}

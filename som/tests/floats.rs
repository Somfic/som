mod common;
use common::*;
use som::source_raw;

// ============================================================================
// Float literals
// ============================================================================

#[test]
fn float_literal() {
    // Return 0 if float works, test compilation
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 3.14;
        0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn float_zero() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 0.0;
        0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

// ============================================================================
// Float arithmetic
// ============================================================================

#[test]
#[ignore = "float arithmetic traits not implemented for f32"]
fn float_add() {
    // 1.5 + 2.5 = 4.0, cast to int for return
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 1.5 + 2.5;
        x as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 4);
}

#[test]
#[ignore = "float arithmetic traits not implemented for f32"]
fn float_sub() {
    // 5.0 - 2.0 = 3.0
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 5.0 - 2.0;
        x as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
#[ignore = "float arithmetic traits not implemented for f32"]
fn float_mul() {
    // 2.0 * 3.0 = 6.0
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 2.0 * 3.0;
        x as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

#[test]
#[ignore = "float arithmetic traits not implemented for f32"]
fn float_div() {
    // 6.0 / 2.0 = 3.0
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 6.0 / 2.0;
        x as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

// ============================================================================
// Float comparisons
// ============================================================================

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_less_than() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if 1.0 < 2.0 {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_greater_than() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if 2.0 > 1.0 {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_equals() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if 1.0 == 1.0 {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_not_equals() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if 1.0 != 2.0 {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_less_than_or_equal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 1.0 <= 2.0;
        let b = 2.0 <= 2.0;
        let c = 3.0 <= 2.0;
        if a && b && !c {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
#[ignore = "float comparison traits not implemented for f32"]
fn float_greater_than_or_equal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 2.0 >= 1.0;
        let b = 2.0 >= 2.0;
        let c = 2.0 >= 3.0;
        if a && b && !c {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

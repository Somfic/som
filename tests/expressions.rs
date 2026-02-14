mod common;
use common::*;
use som::source_raw;

// ============================================================================
// Integer type inference and annotations
// ============================================================================

#[test]
fn infer_i32_literal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 42;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn explicit_i32_annotation() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x: i32 = 42;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
#[ignore = "u8 type not fully supported"]
fn u8_type_annotation() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x: u8 = 255;
        x as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 255);
}

#[test]
#[ignore = "u8 type not fully supported"]
fn u8_to_i32_cast() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x: u8 = 100;
        let y: i32 = x as i32;
        y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
#[ignore = "u8 type not fully supported"]
fn i32_to_u8_truncation() {
    // 256 as u8 should truncate to 0
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x: i32 = 256;
        let y: u8 = x as u8;
        y as i32
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

// ============================================================================
// Boolean operators
// ============================================================================

#[test]
#[ignore = "bitwise & on bool not implemented"]
fn bool_and_true_true() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if true & true {
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
#[ignore = "bitwise & on bool not implemented"]
fn bool_and_true_false() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if true & false {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
#[ignore = "bitwise | on bool not implemented"]
fn bool_or_false_false() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if false | false {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
#[ignore = "bitwise | on bool not implemented"]
fn bool_or_true_false() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if true | false {
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
#[ignore = "unary ! on bool not implemented"]
fn bool_not_true() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if !true {
            1
        } else {
            0
        }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
#[ignore = "unary ! on bool not implemented"]
fn bool_not_false() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if !false {
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
#[ignore = "bitwise operators on bool not implemented"]
fn bool_complex_expression() {
    // (true & false) | true = false | true = true
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        if (true & false) | true {
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

// ============================================================================
// Variable shadowing
// ============================================================================

#[test]
fn shadow_variable() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 1;
        let x = 2;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn shadow_with_different_value() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 10;
        let y = x + 5;
        let x = 100;
        x + y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 115); // 100 + 15
}

// ============================================================================
// Block expressions
// ============================================================================

#[test]
fn block_expression_value() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = {
            let a = 1;
            let b = 2;
            a + b
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn nested_blocks() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = {
            let a = {
                10
            };
            let b = {
                20
            };
            a + b
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn block_scoping() {
    // Inner x should not affect outer x
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 1;
        let y = {
            let x = 100;
            x
        };
        x + y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 101); // outer x (1) + inner x (100)
}

// ============================================================================
// Recursion
// ============================================================================

// Note: Som uses Python-style conditional expressions: `value if cond else other`
// Not Rust-style: `if cond { value } else { other }`

#[test]
fn recursive_factorial() {
    let source = source_raw!(
        r#"
    fn factorial(n: i32) -> i32 {
        1 if n <= 1 else n * factorial(n - 1)
    }

    fn main() -> i32 {
        factorial(5)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 120); // 5! = 120
}

#[test]
fn recursive_fibonacci() {
    let source = source_raw!(
        r#"
    fn fib(n: i32) -> i32 {
        n if n <= 1 else fib(n - 1) + fib(n - 2)
    }

    fn main() -> i32 {
        fib(10)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55); // fib(10) = 55
}

#[test]
fn mutual_recursion() {
    let source = source_raw!(
        r#"
    fn is_even(n: i32) -> bool {
        true if n == 0 else is_odd(n - 1)
    }

    fn is_odd(n: i32) -> bool {
        false if n == 0 else is_even(n - 1)
    }

    fn main() -> i32 {
        1 if is_even(10) else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// Recursion with while loop (avoids if-expression issues)
#[test]
fn recursive_sum_iterative() {
    let source = source_raw!(
        r#"
    fn sum_to_n(n: i32) -> i32 {
        let mut result = 0;
        let mut i = 1;
        while i <= n {
            result = result + i;
            i = i + 1;
        }
        result
    }

    fn main() -> i32 {
        sum_to_n(10)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55); // 1+2+...+10 = 55
}

// ============================================================================
// Nested function calls
// ============================================================================

#[test]
fn nested_function_calls() {
    let source = source_raw!(
        r#"
    fn add_one(x: i32) -> i32 {
        x + 1
    }

    fn double(x: i32) -> i32 {
        x * 2
    }

    fn main() -> i32 {
        double(add_one(5))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 12); // (5 + 1) * 2 = 12
}

#[test]
fn deeply_nested_calls() {
    let source = source_raw!(
        r#"
    fn f(x: i32) -> i32 { x + 1 }
    fn g(x: i32) -> i32 { x * 2 }
    fn h(x: i32) -> i32 { x - 3 }

    fn main() -> i32 {
        h(g(f(10)))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 19); // ((10 + 1) * 2) - 3 = 22 - 3 = 19
}

// ============================================================================
// Arithmetic edge cases
// ============================================================================

#[test]
fn negative_via_subtraction() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        0 - 5
    }
    "#
    );

    let code = compile_and_run(source);
    // Note: exit codes are unsigned, so -5 becomes 251 (256 - 5)
    assert_eq!(code, 251);
}

#[test]
fn arithmetic_precedence() {
    // 2 + 3 * 4 should be 2 + 12 = 14 (not 20)
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        2 + 3 * 4
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 14);
}

#[test]
fn parentheses_override_precedence() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (2 + 3) * 4
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20);
}

#[test]
fn division_truncation() {
    // Integer division should truncate
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        7 / 2
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

// ============================================================================
// Comparison operators (using mutable + if statement pattern)
// ============================================================================

// These tests use Python-style conditional expressions
#[test]
fn compare_equal_expr() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if 5 == 5 else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_not_equal_expr() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if 5 != 3 else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_less_than_expr() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if 3 < 5 else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_greater_than_expr() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if 5 > 3 else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// Working comparison tests using mutable + statement pattern
#[test]
fn compare_equal_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 5 == 5 {
            result = 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_not_equal_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 5 != 3 {
            result = 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_less_than_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 3 < 5 {
            result = 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_greater_than_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 5 > 3 {
            result = 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn compare_less_equal_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 3 <= 5 {
            result = result + 1;
        }
        if 5 <= 5 {
            result = result + 1;
        }
        if 6 <= 5 {
            result = result + 10;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2); // only first two conditions true
}

#[test]
fn compare_greater_equal_statement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 0;
        if 5 >= 3 {
            result = result + 1;
        }
        if 5 >= 5 {
            result = result + 1;
        }
        if 5 >= 6 {
            result = result + 10;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2); // only first two conditions true
}

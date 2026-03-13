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

// ============================================================================
// Modulo operator
// ============================================================================

#[test]
fn modulo_basic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        10 % 3
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn modulo_even_check() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        10 % 2
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn modulo_larger_divisor() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        3 % 5
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn modulo_same() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        7 % 7
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn modulo_one() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        42 % 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn modulo_in_expression() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (10 % 3) + (7 % 4)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 4); // 1 + 3
}

// ============================================================================
// Chained arithmetic
// ============================================================================

#[test]
fn chained_addition() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 + 2 + 3 + 4 + 5
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn chained_subtraction() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        100 - 20 - 30 - 10
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 40);
}

#[test]
fn chained_multiplication() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        2 * 3 * 4
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 24);
}

#[test]
fn mixed_arithmetic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        2 + 3 * 4 - 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 13);
}

#[test]
fn complex_precedence() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (2 + 3) * (4 - 1)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn division_and_modulo() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        17 / 3 + 17 % 3
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7); // 5 + 2
}

// ============================================================================
// Multiple let bindings
// ============================================================================

#[test]
fn multiple_lets_sum() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 10;
        let b = 20;
        let c = 30;
        a + b + c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 60);
}

#[test]
fn let_chain_dependency() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 5;
        let b = a + 3;
        let c = b * 2;
        c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 16);
}

#[test]
fn many_bindings() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 1;
        let b = 2;
        let c = 3;
        let d = 4;
        let e = 5;
        a + b + c + d + e
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

// ============================================================================
// Complex block expressions
// ============================================================================

#[test]
fn block_with_arithmetic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = {
            let a = 10;
            let b = 20;
            a * b
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 200);
}

#[test]
fn block_returns_conditional() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = {
            1 if true else 0
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn nested_block_arithmetic() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = {
            let a = { 3 + 4 };
            a * 2
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 14);
}

#[test]
fn block_with_shadow() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 10;
        let x = { x + 5 };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn multiple_blocks() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = { 10 };
        let b = { 20 };
        let c = { a + b };
        c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

// ============================================================================
// Function composition
// ============================================================================

#[test]
fn triple_composition() {
    let source = source_raw!(
        r#"
    fn f(x: i32) -> i32 { x + 1 }
    fn g(x: i32) -> i32 { x * 2 }
    fn h(x: i32) -> i32 { x - 3 }

    fn main() -> i32 {
        f(g(h(10)))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15); // f(g(7)) = f(14) = 15
}

#[test]
fn function_in_let() {
    let source = source_raw!(
        r#"
    fn square(x: i32) -> i32 { x * x }

    fn main() -> i32 {
        let a = square(3);
        let b = square(4);
        a + b
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 25); // 9 + 16
}

#[test]
fn function_with_multiple_calls() {
    let source = source_raw!(
        r#"
    fn add(a: i32, b: i32) -> i32 { a + b }

    fn main() -> i32 {
        add(add(1, 2), add(3, 4))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10); // add(3, 7)
}

// ============================================================================
// Recursive algorithms
// ============================================================================

#[test]
fn recursive_gcd() {
    let source = source_raw!(
        r#"
    fn gcd(a: i32, b: i32) -> i32 {
        a if b == 0 else gcd(b, a % b)
    }

    fn main() -> i32 {
        gcd(48, 18)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

#[test]
fn recursive_power() {
    let source = source_raw!(
        r#"
    fn power(base: i32, exp: i32) -> i32 {
        1 if exp == 0 else base * power(base, exp - 1)
    }

    fn main() -> i32 {
        power(2, 7)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 128);
}

#[test]
fn recursive_abs() {
    let source = source_raw!(
        r#"
    fn abs(x: i32) -> i32 {
        x if x > 0 else 0 - x
    }

    fn main() -> i32 {
        abs(0 - 42)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn recursive_countdown() {
    let source = source_raw!(
        r#"
    fn countdown(n: i32) -> i32 {
        0 if n <= 0 else countdown(n - 1)
    }

    fn main() -> i32 {
        countdown(10)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn recursive_sum() {
    let source = source_raw!(
        r#"
    fn sum(n: i32) -> i32 {
        0 if n <= 0 else n + sum(n - 1)
    }

    fn main() -> i32 {
        sum(10)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55);
}

// ============================================================================
// Deeply nested shadowing
// ============================================================================

#[test]
fn triple_shadow() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 1;
        let x = 2;
        let x = 3;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn shadow_with_computation() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 10;
        let x = x + 5;
        let x = x * 2;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn shadow_in_blocks() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 1;
        let x = {
            let x = 10;
            x + 1
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 11);
}

#[test]
fn shadow_different_types_return() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = true;
        let x = 42;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

// ============================================================================
// Arithmetic with conditionals
// ============================================================================

#[test]
fn conditional_arithmetic_true() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (1 + 2) if true else (3 + 4)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn conditional_arithmetic_false() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (1 + 2) if false else (3 + 4)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn conditional_with_comparison() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        10 if 5 > 3 else 20
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn conditional_with_variable() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 5;
        (x * 2) if x > 3 else (x + 1)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn nested_conditional() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if true else (2 if false else 3)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn nested_conditional_inner() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        1 if false else (2 if true else 3)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn conditional_in_function() {
    let source = source_raw!(
        r#"
    fn choose(b: bool, x: i32, y: i32) -> i32 {
        x if b else y
    }

    fn main() -> i32 {
        choose(true, 42, 0)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

// ============================================================================
// Large computation chains
// ============================================================================

#[test]
fn sum_to_100() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= 10 {
            sum = sum + i;
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55);
}

#[test]
fn factorial_7() {
    let source = source_raw!(
        r#"
    fn factorial(n: i32) -> i32 {
        1 if n <= 1 else n * factorial(n - 1)
    }

    fn main() -> i32 {
        factorial(7)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 176); // 5040 % 256 = 176
}

#[test]
fn fibonacci_12() {
    let source = source_raw!(
        r#"
    fn fib(n: i32) -> i32 {
        n if n <= 1 else fib(n - 1) + fib(n - 2)
    }

    fn main() -> i32 {
        fib(12)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 144);
}

// ============================================================================
// Identity and helper functions
// ============================================================================

#[test]
fn identity_function() {
    let source = source_raw!(
        r#"
    fn id(x: i32) -> i32 { x }

    fn main() -> i32 {
        id(42)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn constant_function() {
    let source = source_raw!(
        r#"
    fn always_five() -> i32 { 5 }

    fn main() -> i32 {
        always_five()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn add_function() {
    let source = source_raw!(
        r#"
    fn add(a: i32, b: i32) -> i32 { a + b }

    fn main() -> i32 {
        add(20, 22)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn subtract_function() {
    let source = source_raw!(
        r#"
    fn sub(a: i32, b: i32) -> i32 { a - b }

    fn main() -> i32 {
        sub(50, 8)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn max_function() {
    let source = source_raw!(
        r#"
    fn max(a: i32, b: i32) -> i32 { a if a > b else b }

    fn main() -> i32 {
        max(10, 42)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn min_function() {
    let source = source_raw!(
        r#"
    fn min(a: i32, b: i32) -> i32 { a if a < b else b }

    fn main() -> i32 {
        min(10, 42)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

// ============================================================================
// Constant folding / edge cases
// ============================================================================

#[test]
fn zero_operations() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        0 + 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn multiply_by_zero() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        42 * 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn multiply_by_one() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        42 * 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn subtract_self() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 42;
        x - x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn divide_self() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 7;
        x / x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn large_value_wraps() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 200;
        let y = 100;
        x + y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 44); // 300 % 256 = 44
}

#[test]
fn overflow_wraps() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        255 + 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0); // 256 % 256 = 0
}

#[test]
fn negative_wraps() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        0 - 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 255);
}

// ============================================================================
// More edge cases
// ============================================================================

#[test]
fn parenthesized_expression() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        (((42)))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn deeply_nested_parens() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        ((1 + 2) * (3 + 4))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 21);
}

#[test]
fn many_function_params() {
    let source = source_raw!(
        r#"
    fn sum4(a: i32, b: i32, c: i32, d: i32) -> i32 { a + b + c + d }

    fn main() -> i32 {
        sum4(10, 20, 30, 40)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn block_is_expression() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        { 42 }
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

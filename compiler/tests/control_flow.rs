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

// ============================================================================
// While patterns
// ============================================================================

#[test]
fn while_countdown() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 10;
        while x > 0 {
            x = x - 1;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn while_accumulator() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= 5 {
            sum = sum + i;
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn while_double_accumulator() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut product = 1;
        let mut i = 1;
        while i <= 5 {
            sum = sum + i;
            product = product * i;
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn while_multiple_in_sequence() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut total = 0;
        let mut i = 1;
        while i <= 5 {
            total = total + i;
            i = i + 1;
        }
        let mut j = 1;
        while j <= 3 {
            total = total + j;
            j = j + 1;
        }
        total
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 21); // 15 + 6
}

#[test]
fn while_with_function_call() {
    let source = source_raw!(
        r#"
    fn double(x: i32) -> i32 {
        x * 2
    }

    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= 4 {
            sum = sum + double(i);
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20); // 2+4+6+8
}

#[test]
fn while_complex_condition() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        let mut count = 0;
        while x < 10 {
            x = x + 2;
            count = count + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn while_fibonacci() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 0;
        let mut b = 1;
        let mut i = 0;
        while i < 10 {
            let tmp = a + b;
            a = b;
            b = tmp;
            i = i + 1;
        }
        a
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55);
}

#[test]
fn while_factorial() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 1;
        let mut i = 1;
        while i <= 5 {
            result = result * i;
            i = i + 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 120);
}

#[test]
fn while_early_exit_via_condition() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 100;
        let mut count = 0;
        while x > 50 {
            x = x - 20;
            count = count + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3); // 100->80->60->40, 3 iterations
}

#[test]
fn while_nested_three_deep() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 0;
        let mut i = 0;
        while i < 3 {
            let mut j = 0;
            while j < 3 {
                let mut k = 0;
                while k < 3 {
                    count = count + 1;
                    k = k + 1;
                }
                j = j + 1;
            }
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 27); // 3 * 3 * 3
}

// ============================================================================
// If/else patterns
// ============================================================================

#[test]
fn if_deeply_nested() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 5;
        let mut result = 0;
        if x > 0 {
            if x > 3 {
                if x > 4 {
                    result = 42;
                } else {
                    result = 30;
                }
            } else {
                result = 20;
            }
        } else {
            result = 10;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn if_with_function_call() {
    let source = source_raw!(
        r#"
    fn is_big(x: i32) -> i32 {
        let mut r = 0;
        if x > 10 {
            r = 1;
        }
        r
    }

    fn main() -> i32 {
        let mut result = 0;
        if is_big(20) == 1 {
            result = 50;
        } else {
            result = 10;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 50);
}

#[test]
fn if_comparing_variables() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let a = 5;
        let b = 3;
        let mut result = 0;
        if a > b {
            result = a;
        } else {
            result = b;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn if_multiple_conditions_sequence() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 15;
        let mut result = 0;
        if x > 10 {
            result = result + 1;
        }
        if x > 5 {
            result = result + 2;
        }
        if x > 20 {
            result = result + 4;
        }
        if x == 15 {
            result = result + 8;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 11); // 1 + 2 + 8
}

#[test]
fn if_inside_while() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut even_count = 0;
        let mut i = 0;
        while i < 10 {
            if i % 2 == 0 {
                even_count = even_count + 1;
            }
            i = i + 1;
        }
        even_count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5); // 0, 2, 4, 6, 8
}

#[test]
fn if_else_chain_four() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 3;
        let mut result = 0;
        if x == 1 {
            result = 10;
        } else if x == 2 {
            result = 20;
        } else if x == 3 {
            result = 30;
        } else {
            result = 40;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn if_with_block_result() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 7;
        let mut category = 0;
        if x > 5 {
            category = 2;
        } else {
            category = 1;
        }
        let result = category * 10;
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20);
}

#[test]
fn if_comparison_equal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 5;
        let mut result = 0;
        if x == 5 {
            result = 99;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 99);
}

#[test]
fn if_comparison_not_equal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 7;
        let mut result = 0;
        if x != 0 {
            result = 88;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 88);
}

#[test]
fn if_comparison_less_equal() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let x = 10;
        let mut result = 0;
        if x <= 10 {
            result = 77;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 77);
}

// ============================================================================
// While computing patterns
// ============================================================================

#[test]
fn while_count_digits() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut n = 12345;
        let mut digits = 0;
        while n > 0 {
            n = n / 10;
            digits = digits + 1;
        }
        digits
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn while_power_of_two() {
    // 2^8 = 256, wraps to 0 as u8
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut result = 1;
        let mut i = 0;
        while i < 8 {
            result = result * 2;
            i = i + 1;
        }
        result
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0); // 256 % 256 = 0
}

#[test]
fn while_find_first_multiple() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut n = 50;
        while n % 7 != 0 {
            n = n + 1;
        }
        n
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 56);
}

#[test]
fn while_collatz_steps() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut n = 6;
        let mut steps = 0;
        while n != 1 {
            if n % 2 == 0 {
                n = n / 2;
            } else {
                n = n * 3 + 1;
            }
            steps = steps + 1;
        }
        steps
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 8); // 6->3->10->5->16->8->4->2->1
}

// ============================================================================
// Combined patterns
// ============================================================================

#[test]
fn while_with_conditional_increment() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 0;
        while i < 10 {
            if i % 2 == 0 {
                sum = sum + i;
            } else {
                sum = sum + 1;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 25); // evens: 0+2+4+6+8=20, odds: 5*1=5, total=25
}

#[test]
fn nested_if_in_while_with_accumulator() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= 10 {
            if i % 3 == 0 {
                sum = sum + 10;
            } else if i % 2 == 0 {
                sum = sum + 5;
            } else {
                sum = sum + 1;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    // i=1:+1, i=2:+5, i=3:+10, i=4:+5, i=5:+1, i=6:+10, i=7:+1, i=8:+5, i=9:+10, i=10:+5
    // = 1+5+10+5+1+10+1+5+10+5 = 53
    assert_eq!(code, 53);
}

#[test]
fn while_with_comparison_chain() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 0;
        let mut i = 0;
        while i <= 20 {
            if i >= 5 {
                if i <= 15 {
                    count = count + 1;
                }
            }
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 11); // 5,6,7,8,9,10,11,12,13,14,15
}

#[test]
fn if_else_returns_from_function() {
    let source = source_raw!(
        r#"
    fn classify(x: i32) -> i32 {
        let mut r = 0;
        if x > 10 {
            r = 3;
        } else if x > 5 {
            r = 2;
        } else {
            r = 1;
        }
        r
    }

    fn main() -> i32 {
        let a = classify(15);
        let b = classify(7);
        let c = classify(2);
        a * 100 + b * 10 + c
    }
    "#
    );

    let code = compile_and_run(source);
    // 3*100 + 2*10 + 1 = 321, but u8 wraps: 321 % 256 = 65
    assert_eq!(code, 65);
}

#[test]
fn multiple_functions_with_control_flow() {
    let source = source_raw!(
        r#"
    fn abs_diff(a: i32, b: i32) -> i32 {
        let mut result = 0;
        if a > b {
            result = a - b;
        } else {
            result = b - a;
        }
        result
    }

    fn sum_to(n: i32) -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= n {
            sum = sum + i;
            i = i + 1;
        }
        sum
    }

    fn main() -> i32 {
        let x = sum_to(5);
        let y = sum_to(3);
        abs_diff(x, y)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 9); // |15 - 6| = 9
}

#[test]
fn while_gcd_iterative() {
    let source = source_raw!(
        r#"
    fn gcd(a: i32, b: i32) -> i32 {
        let mut x = a;
        let mut y = b;
        while y != 0 {
            let t = y;
            y = x % y;
            x = t;
        }
        x
    }

    fn main() -> i32 {
        gcd(48, 18)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

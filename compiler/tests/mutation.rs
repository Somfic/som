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

// ============================================================================
// Swap pattern
// ============================================================================

#[test]
fn swap_variables() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 10;
        let mut b = 20;
        let tmp = a;
        a = b;
        b = tmp;
        a
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20);
}

#[test]
fn swap_three() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 10;
        let mut b = 20;
        let mut c = 30;
        let tmp = a;
        a = b;
        b = c;
        c = tmp;
        a + b + c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 60); // 20 + 30 + 10
}

// ============================================================================
// Counter patterns
// ============================================================================

#[test]
fn counter_increment() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 0;
        let mut i = 0;
        while i < 5 {
            count = count + 1;
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn counter_decrement() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 10;
        let mut i = 0;
        while i < 5 {
            count = count - 1;
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn counter_by_step() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 0;
        let mut i = 0;
        while i < 5 {
            count = count + 3;
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

// ============================================================================
// Accumulator with conditionals
// ============================================================================

#[test]
fn accumulate_evens() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 0;
        while i < 10 {
            if i % 2 == 0 {
                sum = sum + i;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20); // 0+2+4+6+8
}

#[test]
fn accumulate_with_if() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 1;
        while i <= 10 {
            if i > 5 {
                sum = sum + i;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 40); // 6+7+8+9+10
}

#[test]
fn conditional_increment() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        if true {
            x = x + 1;
        }
        if false {
            x = x + 10;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// ============================================================================
// Mutation in nested blocks
// ============================================================================

#[test]
fn mut_in_block() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        {
            x = 42;
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn mut_in_nested_blocks() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 0;
        {
            {
                x = 10;
            };
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn mut_preserved_after_block() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 5;
        {
            let y = x + 1;
            x = y;
        };
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

// ============================================================================
// Multiple mutable variables interacting
// ============================================================================

#[test]
fn two_vars_swap_sum() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 3;
        let mut b = 7;
        let tmp = a;
        a = b;
        b = tmp;
        a + b
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn three_mutable_vars() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 1;
        let mut b = 2;
        let mut c = 3;
        a = a + b;
        b = b + c;
        c = c + a;
        a + b + c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 14); // (1+2) + (2+3) + (3+3) = 3+5+6
}

#[test]
fn cascade_assignments() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 1;
        let mut b = 0;
        let mut c = 0;
        b = a + 1;
        c = b + 1;
        a = c + 1;
        a
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 4);
}

#[test]
fn accumulate_two_vars() {
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
        sum + product
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 135); // sum=15, product=120, 15+120=135
}

// ============================================================================
// Mutation with function return values
// ============================================================================

#[test]
fn mut_from_function() {
    let source = source_raw!(
        r#"
    fn compute() -> i32 {
        42
    }

    fn main() -> i32 {
        let mut x = 0;
        x = compute();
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn mut_add_function_result() {
    let source = source_raw!(
        r#"
    fn five() -> i32 {
        5
    }

    fn main() -> i32 {
        let mut x = 10;
        x = x + five();
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn mut_in_loop_with_function() {
    let source = source_raw!(
        r#"
    fn double(x: i32) -> i32 {
        x * 2
    }

    fn main() -> i32 {
        let mut x = 1;
        let mut i = 0;
        while i < 3 {
            x = double(x);
            i = i + 1;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 8);
}

// ============================================================================
// Mutation in while loop body with complex logic
// ============================================================================

#[test]
fn fibonacci_mutation() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 0;
        let mut b = 1;
        let mut i = 0;
        while i < 10 {
            let tmp = b;
            b = a + b;
            a = tmp;
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
fn bubble_sort_two() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut a = 20;
        let mut b = 10;
        if a > b {
            let tmp = a;
            a = b;
            b = tmp;
        }
        a
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn counting_pattern() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut count = 0;
        let mut i = 0;
        while i < 20 {
            if i % 3 == 0 {
                count = count + 1;
            }
            i = i + 1;
        }
        count
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7); // 0,3,6,9,12,15,18
}

#[test]
fn alternating_accumulator() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut sum = 100;
        let mut i = 0;
        while i < 10 {
            if i % 2 == 0 {
                sum = sum + i;
            } else {
                sum = sum - i;
            }
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 95); // 100 + (0-1+2-3+4-5+6-7+8-9) = 100 + (-5) = 95
}

#[test]
fn mutation_chain_in_while() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 1;
        while x < 100 {
            x = x * 2;
        }
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 128);
}

#[test]
fn mut_with_modulo() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 100;
        x = x % 7;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn complex_mutation_sequence() {
    let source = source_raw!(
        r#"
    fn main() -> i32 {
        let mut x = 10;
        let mut y = 3;
        x = x * y;
        y = x - y;
        x = x + y;
        x = x % 100;
        x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 57); // x=30, y=27, x=57, x=57%100=57
}

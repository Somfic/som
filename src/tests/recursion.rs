use crate::tests::interpret;

#[test]
fn test_factorial_recursion() {
    let program = r#"
        let factorial = fn(n ~ int) -> int {
            1 if n < 2 else n * factorial(n - 1)
        };
        factorial(5)
    "#;
    assert_eq!(120, interpret(program));
}

#[test]
fn test_factorial_zero() {
    let program = r#"
        let factorial = fn(n ~ int) -> int {
            1 if n < 2 else n * factorial(n - 1)
        };
        factorial(0)
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn test_factorial_one() {
    let program = r#"
        let factorial = fn(n ~ int) -> int {
            1 if n < 2 else n * factorial(n - 1)
        };
        factorial(1)
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn test_fibonacci_recursion() {
    let program = r#"
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };
        fib(10)
    "#;
    assert_eq!(55, interpret(program));
}

#[test]
fn test_fibonacci_small_values() {
    let program = r#"
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };
        fib(0)
    "#;
    assert_eq!(0, interpret(program));

    let program = r#"
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };
        fib(1)
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn test_sum_to_n_recursion() {
    let program = r#"
        let sum = fn(n ~ int) -> int {
            0 if n < 1 else n + sum(n - 1)
        };
        sum(10)
    "#;
    assert_eq!(55, interpret(program));
}

#[test]
fn test_countdown_recursion() {
    let program = r#"
        let countdown = fn(n ~ int) -> int {
            n if n < 1 else countdown(n - 1)
        };
        countdown(10)
    "#;
    assert_eq!(0, interpret(program));
}

#[test]
fn test_power_recursion() {
    let program = r#"
        let power = fn(base ~ int, exp ~ int) -> int {
            1 if exp < 1 else base * power(base, exp - 1)
        };
        power(2, 10)
    "#;
    assert_eq!(1024, interpret(program));
}

#[test]
fn test_gcd_recursion() {
    let program = r#"
        let gcd = fn(a ~ int, b ~ int) -> int {
            a if b == 0 else gcd(b, a - (a / b) * b)
        };
        gcd(48, 18)
    "#;
    assert_eq!(6, interpret(program));
}

#[test]
fn test_multiply_by_addition_recursion() {
    let program = r#"
        let multiply = fn(a ~ int, b ~ int) -> int {
            0 if b < 1 else a + multiply(a, b - 1)
        };
        multiply(7, 8)
    "#;
    assert_eq!(56, interpret(program));
}

#[test]
fn test_tail_recursive_factorial() {
    let program = r#"
        let factorial = fn(n ~ int) -> int {
            let helper = fn(n ~ int, acc ~ int) -> int {
                acc if n < 2 else helper(n - 1, n * acc)
            };
            helper(n, 1)
        };
        factorial(5)
    "#;
    assert_eq!(120, interpret(program));
}

#[test]
fn test_tail_recursive_sum() {
    let program = r#"
        let sum = fn(n ~ int) -> int {
            let helper = fn(n ~ int, acc ~ int) -> int {
                acc if n < 1 else helper(n - 1, n + acc)
            };
            helper(n, 0)
        };
        sum(100)
    "#;
    assert_eq!(5050, interpret(program));
}

#[test]
fn test_mutual_recursion_even_odd() {
    let program = r#"
        let is_even = fn(n ~ int) -> int {
            let is_odd = fn(n ~ int) -> int {
                0 if n == 0 else is_even(n - 1)
            };
            1 if n == 0 else is_odd(n - 1)
        };
        is_even(10)
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn test_recursive_with_multiple_calls() {
    let program = r#"
        let ackermann = fn(m ~ int, n ~ int) -> int {
            (n + 1) if m == 0 else (
                ackermann(m - 1, 1) if n == 0 else
                ackermann(m - 1, ackermann(m, n - 1))
            )
        };
        ackermann(2, 2)
    "#;
    assert_eq!(7, interpret(program));
}

#[test]
fn test_recursive_length_calculation() {
    let program = r#"
        let length = fn(n ~ int) -> int {
            0 if n < 1 else 1 + length(n - 1)
        };
        length(42)
    "#;
    assert_eq!(42, interpret(program));
}

#[test]
fn test_recursive_with_conditionals() {
    let program = r#"
        let collatz = fn(n ~ int) -> int {
            1 if n == 1 else (
                collatz(n / 2) if n - (n / 2) * 2 == 0 else
                collatz(n * 3 + 1)
            )
        };
        collatz(10)
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn test_nested_recursive_calls() {
    let program = r#"
        let nested = fn(n ~ int) -> int {
            n if n < 2 else nested(nested(n - 1)) + 1
        };
        nested(4)
    "#;
    assert_eq!(4, interpret(program));
}

#[test]
fn test_recursion_with_arithmetic() {
    let program = r#"
        let triangle = fn(n ~ int) -> int {
            0 if n < 1 else n + triangle(n - 1)
        };
        triangle(7)
    "#;
    assert_eq!(28, interpret(program));
}

// Edge case tests for tail-call optimization

#[test]
fn test_tail_recursion_nested_conditionals() {
    let program = r#"
        let count = fn(n ~ int, acc ~ int) -> int {
            (acc if n < 5 else count(n - 1, acc + 1)) if n < 1 else count(n - 1, acc + 2)
        };
        count(10, 0)
    "#;
    assert_eq!(20, interpret(program));
}

#[test]
fn test_tail_recursion_argument_swapping() {
    let program = r#"
        let swap_sum = fn(a ~ int, b ~ int, n ~ int) -> int {
            a if n < 1 else swap_sum(b, a + b, n - 1)
        };
        swap_sum(0, 1, 10)
    "#;
    assert_eq!(55, interpret(program));
}

#[test]
fn test_tail_recursion_complex_expressions() {
    let program = r#"
        let calc = fn(n ~ int, acc ~ int) -> int {
            acc if n < 1 else calc(n - 2, acc + n * 2)
        };
        calc(10, 0)
    "#;
    assert_eq!(60, interpret(program));
}

#[test]
fn test_tail_recursion_in_blocks() {
    let program = r#"
        let count_blocks = fn(n ~ int, acc ~ int) -> int {
            {
                let temp = n + 1;
                let result = acc + temp;
                result if n < 1 else count_blocks(n - 1, result)
            }
        };
        count_blocks(5, 0)
    "#;
    assert_eq!(21, interpret(program));
}

#[test]
fn test_tail_recursion_very_deep() {
    let program = r#"
        let deep = fn(n ~ int, acc ~ int) -> int {
            acc if n < 1 else deep(n - 1, acc + 1)
        };
        deep(10000, 0)
    "#;
    assert_eq!(10000, interpret(program));
}

#[test]
fn test_tail_recursion_with_boolean_ops() {
    let program = r#"
        let bool_count = fn(n ~ int, acc ~ int) -> int {
            acc if n == 0 else bool_count(n - 1, acc + 1)
        };
        bool_count(100, 0)
    "#;
    assert_eq!(100, interpret(program));
}

#[test]
fn test_tail_recursion_all_params_changing() {
    let program = r#"
        let multi_change = fn(a ~ int, b ~ int, c ~ int) -> int {
            a + b + c if a < 1 else multi_change(a - 1, b + 1, c + 2)
        };
        multi_change(5, 0, 0)
    "#;
    assert_eq!(15, interpret(program));
}

#[test]
fn test_tail_recursion_constant_params() {
    let program = r#"
        let const_param = fn(n ~ int, acc ~ int, constant ~ int) -> int {
            acc if n < 1 else const_param(n - 1, acc + constant, constant)
        };
        const_param(10, 0, 7)
    "#;
    assert_eq!(70, interpret(program));
}

#[test]
fn test_tail_recursion_negative_numbers() {
    let program = r#"
        let countdown = fn(n ~ int, acc ~ int) -> int {
            acc if n == 0 else countdown(n + 1, acc + 1)
        };
        countdown(0 - 5, 0)
    "#;
    assert_eq!(5, interpret(program));
}

#[test]
fn test_tail_recursion_single_iteration() {
    let program = r#"
        let single = fn(n ~ int, acc ~ int) -> int {
            acc if n < 1 else single(n - 1, acc + 100)
        };
        single(1, 0)
    "#;
    assert_eq!(100, interpret(program));
}

#[test]
fn test_tail_recursion_greater_than_equal() {
    let program = r#"
        let gte_count = fn(n ~ int, target ~ int, acc ~ int) -> int {
            acc if n == target else gte_count(n + 1, target, acc + 1)
        };
        gte_count(0, 50, 0)
    "#;
    assert_eq!(50, interpret(program));
}

#[test]
fn test_tail_recursion_multiple_operations() {
    let program = r#"
        let ops = fn(n ~ int, acc ~ int) -> int {
            acc if n < 1 else ops(n - 1, (acc + n) * 2 - n)
        };
        ops(5, 0)
    "#;
    assert_eq!(129, interpret(program));
}

#[test]
fn test_tail_recursion_conditional_in_base_case() {
    let program = r#"
        let cond_base = fn(n ~ int, acc ~ int) -> int {
            (100 if acc > 50 else acc) if n < 1 else cond_base(n - 1, acc + n)
        };
        cond_base(10, 0)
    "#;
    assert_eq!(100, interpret(program));
}

#[test]
fn test_tail_recursion_alternating_operations() {
    let program = r#"
        let alt = fn(n ~ int, acc ~ int, toggle ~ int) -> int {
            acc if n < 1 else (
                alt(n - 1, acc + n, 0) if toggle == 1 else alt(n - 1, acc - n, 1)
            )
        };
        alt(10, 100, 0)
    "#;
    assert_eq!(95, interpret(program));
}

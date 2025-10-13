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

use super::run_and_assert;

#[test]
fn return_value() {
    let source_code = "fn one() 1 fn main() one()";
    let expected = 1;

    run_and_assert(source_code, expected);
}

#[test]
fn parameter_return_value() {
    let source_code = "fn number(a ~ int) a fn main() number(1)";
    let expected = 1;

    run_and_assert(source_code, expected);
}

#[test]
fn rescursion() {
    let source_code =
        "fn fib(n ~ int) ~ int n if n < 2 else fib(n - 1) + fib(n - 2) fn main() fib(10)";
    let expected = 55;

    run_and_assert(source_code, expected);
}

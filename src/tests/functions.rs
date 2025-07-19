use crate::tests::interpret;

#[test]
fn function_definition_and_call() {
    assert_eq!(5, interpret("let add = fn(a ~ i32, b ~ i32) -> i32 { a + b }; add(2, 3)"));
    assert_eq!(10, interpret("let multiply = fn(x ~ i32, y ~ i32) -> i32 { x * y }; multiply(2, 5)"));
}

#[test]
fn function_with_no_parameters() {
    assert_eq!(42, interpret("let get_answer = fn() -> i32 { 42 }; get_answer()"));
    assert_eq!(1, interpret("let get_true = fn() -> bool { true }; get_true()"));
}

#[test]
fn function_returning_function_result() {
    assert_eq!(
        6,
        interpret("let add = fn(a ~ i32, b ~ i32) -> i32 { a + b }; let double = fn(x ~ i32) -> i32 { add(x, x) }; double(3)")
    );
}

#[test]
fn function_with_complex_body() {
    assert_eq!(
        15,
        interpret("let sum_to_n = fn(n ~ i32) -> i32 { let sum = 0; let i = 1; { sum = sum + i; i = i + 1; } if i <= n else sum }; sum_to_n(5)")
    );
}

#[test]
fn function_with_boolean_parameters() {
    assert_eq!(
        1,
        interpret("let and_fn = fn(a ~ bool, b ~ bool) -> bool { a if b else false }; and_fn(true, true)")
    );
    assert_eq!(
        0,
        interpret("let and_fn = fn(a ~ bool, b ~ bool) -> bool { a if b else false }; and_fn(true, false)")
    );
}

#[test]
fn function_variable_declaration() {
    assert_eq!(
        100,
        interpret("let square = fn(x ~ i32) -> i32 { x * x }; let result = square(10); result")
    );
}

#[test]
fn nested_function_calls() {
    assert_eq!(
        8,
        interpret("let add = fn(a ~ i32, b ~ i32) -> i32 { a + b }; add(add(2, 3), 3)")
    );
    assert_eq!(
        27,
        interpret("let triple = fn(x ~ i32) -> i32 { x * 3 }; triple(triple(3))")
    );
}

#[test]
fn function_with_unit_return() {
    // Functions that don't explicitly return a value should return unit
    assert_eq!(
        0, // Assuming unit is represented as 0 in the test framework
        interpret("let do_nothing = fn() { let x = 5; }; do_nothing()")
    );
}

use crate::tests::interpret;

#[test]
fn type_annotations_basic() {
    assert_eq!(5, interpret("let x = 5; x"));
    assert_eq!(1, interpret("let flag = true; flag"));
    assert_eq!(10, interpret("let number = 10; number"));
}

#[test]
fn type_annotations_with_arithmetic() {
    assert_eq!(15, interpret("let a = 5; let b = 10; a + b"));
    assert_eq!(20, interpret("let x = 4; let y = 5; x * y"));
}

#[test]
fn explicit_type_matching() {
    // Test that explicit types match the inferred types
    assert_eq!(42, interpret("let answer = 42; answer"));
    assert_eq!(0, interpret("let flag = false; flag"));
    assert_eq!(123, interpret("let big_number = 123; big_number"));
}

#[test]
fn function_parameter_types() {
    assert_eq!(
        8,
        interpret("let add ~ fn(i32, i32) -> i32 = fn(a ~ i32, b ~ i32) -> i32 { a + b }; add(3, 5)")
    );
}

#[test]
fn function_return_types() {
    assert_eq!(
        25,
        interpret("let square = fn(x ~ i32) -> i32 { x * x }; square(5)")
    );
    assert_eq!(
        1,
        interpret("let is_positive = fn(n ~ i32) -> bool { true }; is_positive(5)")
    );
}

#[test]
fn mixed_integer_types() {
    // Test that i32 and i64 are properly handled
    assert_eq!(5, interpret("let small ~ i32 = 5; small"));
    assert_eq!(5, interpret("let big ~ i64 = 5; big"));
}

#[test]
fn type_in_conditionals() {
    assert_eq!(10, interpret("let result ~ i32 = 5 if false else 10; result"));
    assert_eq!(1, interpret("let flag ~ bool = true if true else false; flag"));
}

#[test]
fn type_in_blocks() {
    assert_eq!(15, interpret("let value ~ i32 = { let temp = 10; temp + 5 }; value"));
    assert_eq!(0, interpret("let flag ~ bool = { false }; flag"));
}

#[test]
fn complex_type_scenarios() {
    assert_eq!(
        30,
        interpret("let compute ~ fn(i32) -> i32 = fn(x ~ i32) -> i32 { x * 2 }; let result ~ i32 = compute(15); result")
    );
}

// Note: Error cases would be tested in a separate error handling test file
// since they would need different assertion patterns

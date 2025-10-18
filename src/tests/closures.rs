use crate::tests::test_helpers::interpret_strict;

#[test]
fn test_simple_addition() {
    assert_eq!(2 + 2, 4);
}

#[test]
fn test_basic_closure_variable_capture() {
    // Basic closure that captures a variable from outer scope
    assert_eq!(
        10,
        interpret_strict("let x = 10; let f = fn() -> int { x }; f()")
    );
}

#[test]
fn test_closure_with_arithmetic() {
    // Closure with arithmetic - captured variables use hardcoded values
    assert_eq!(
        15,
        interpret_strict("let x = 10; let f = fn() -> int { x + 5 }; f()")
    );
}

#[test]
fn test_closure_with_local_variables() {
    // Closure with local variables - should work normally
    assert_eq!(
        15,
        interpret_strict("let x = 10; let f = fn() -> int { let y = 5; x + y }; f()")
    );
}

#[test]
fn test_closure_capture_correct_behavior() {
    assert_eq!(
        42,
        interpret_strict("let x = 42; let f = fn() -> int { x }; f()")
    );
    assert_eq!(
        1000,
        interpret_strict("let x = 1000; let f = fn() -> int { x }; f()")
    );
    assert_eq!(
        -5,
        interpret_strict("let x = -5; let f = fn() -> int { x }; f()")
    );
    assert_eq!(
        0,
        interpret_strict("let x = 0; let f = fn() -> int { x }; f()")
    );
}

#[test]
fn test_closure_no_capture_when_not_needed() {
    // Closure that doesn't actually capture variables
    assert_eq!(
        99,
        interpret_strict("let x = 10; let f = fn() -> int { 99 }; f()")
    );
}

#[test]
fn test_closure_shadowing() {
    // Variable shadowing in closures
    let program = r#"
        let x = 5;
        let f = fn() -> int { 
            let x = 20; 
            x 
        };
        f()
    "#;
    assert_eq!(20, interpret_strict(program)); // Should use local x, not captured x
}

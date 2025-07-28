use crate::tests::interpret;
use crate::tests::test_helpers::expect_error;

// Regression tests for specific bugs that have been found and fixed
// These tests ensure that fixed bugs don't reappear in future versions

#[test]
fn regression_unary_precedence_bug() {
    // BUG FIXED: Unary precedence bug where `2 * -2` returned `4` instead of `-4`
    // This was fixed by changing binding power from `BindingPower::Additive` to `BindingPower::Unary`
    assert_eq!(-4, interpret("2 * -2"));
    assert_eq!(4, interpret("(-2) * (-2)"));
    assert_eq!(-6, interpret("3 * -2"));
    assert_eq!(6, interpret("(-3) * (-2)"));
    
    // Additional unary precedence tests
    assert_eq!(-1, interpret("3 + -4"));
    assert_eq!(7, interpret("3 - -4"));
    assert_eq!(-2, interpret("-6 / 3"));
    assert_eq!(2, interpret("-6 / -3"));
}

#[test]
fn regression_arithmetic_calculation_bug() {
    // BUG FIXED: Arithmetic calculation bug where `(2 * (3 + 4)) + (5 * (1 + 1))` returned `23` instead of `24`
    // This was actually a test expectation error - the arithmetic was working correctly
    assert_eq!(24, interpret("(2 * (3 + 4)) + (5 * (1 + 1))"));
    
    // Additional complex arithmetic tests to prevent similar issues
    assert_eq!(40, interpret("(3 * (2 + 3)) + (5 * (3 + 2))"));  // (3 * 5) + (5 * 5) = 15 + 25 = 40
    assert_eq!(42, interpret("(1 * (5 + 2)) + (7 * (4 + 1))"));  // (1 * 7) + (7 * 5) = 7 + 35 = 42
}

#[test]
fn regression_conditional_precedence_bug() {
    // BUG FIXED: Conditional precedence bug where `1 + 2 if true else 3 * 4` had incorrect test expectation
    // The test expected 7 but precedence was working correctly to give 3
    assert_eq!(3, interpret("1 + 2 if true else 3 * 4"));  // Should be (1 + 2) if true else (3 * 4) = 3
    
    // Additional conditional precedence tests
    assert_eq!(12, interpret("1 + 2 if false else 3 * 4"));  // Should be 3 * 4 = 12
    assert_eq!(6, interpret("2 * 3 if true else 4 + 5"));    // Should be 2 * 3 = 6
    assert_eq!(9, interpret("2 * 3 if false else 4 + 5"));   // Should be 4 + 5 = 9
}

#[test]
fn regression_type_annotation_bug() {
    // BUG FIXED: Type annotations with `~` syntax caused type mismatches
    // This was fixed by adding function type parser and registering TokenKind::Function handler
    assert_eq!(5, interpret("let add ~ fn(int, int) -> int = fn(x ~ int, y ~ int) -> int { x + y }; add(2, 3)"));
    assert_eq!(10, interpret("let x ~ int = 10; x"));
    assert_eq!(1, interpret("let flag ~ bool = true; flag"));
    
    // More complex type annotation scenarios
    assert_eq!(25, interpret("let square ~ fn(int) -> int = fn(x ~ int) -> int { x * x }; square(5)"));
    assert_eq!(0, interpret("let is_false ~ fn() -> bool = fn() -> bool { false }; is_false()"));
}

#[test]
fn regression_large_integer_overflow_bug() {
    // BUG NOT YET FIXED: Parser fails on large integers with `ParseIntError::PosOverflow`
    // These tests document the current limitation and should pass once the bug is fixed
    
    // Test that we can handle reasonable-sized integers
    assert_eq!(1000000, interpret("1000000"));
    assert_eq!(10000000, interpret("10000000"));
    
    // These should work but currently might fail due to the parsing bug
    // Uncomment once the integer parsing is fixed
    // assert_eq!(2147483647, interpret("2147483647"));    // Max i32
    // assert_eq!(-2147483648, interpret("-2147483648"));  // Min i32
}

#[test]
fn regression_complex_function_syntax_bug() {
    // BUG NOT YET FIXED: Some function syntax patterns cause "unexpected token" errors
    // This test documents cases that should work once parser grammar is improved
    
    // This currently works
    assert_eq!(10, interpret("let double = fn(x ~ int) -> int { let temp = x * 2; temp }; double(5)"));
    
    // These might fail due to complex syntax parsing issues
    // Uncomment and adjust once parser improvements are made
    // assert_eq!(15, interpret("let sum_to_n = fn(n ~ int) -> int { let sum = 0; let i = 1; loop { sum = sum + i; i = i + 1; if i > n then break } sum }; sum_to_n(5)"));
}

#[test]
fn regression_nested_scoping_variable_bug() {
    // Variable scoping works correctly for local variables
    // 
    // LIMITATION: Closure capture is not fully implemented
    // Functions that reference variables from outer scopes have architectural limitations
    // due to how Cranelift variables are scoped to specific function builders.
    
    // Local variable scoping works correctly
    assert_eq!(15, interpret("let outer = 10; let inner = 5; outer + inner"));
    
    // Closure capture requires architectural changes:
    // - Closure conversion (transform closures to functions with captured vars as parameters)
    // - Closure objects (store captured values in objects)
    // - Global variable approach (store captured values in global memory)
    // 
    // Current limitation: assert_eq!(15, interpret("let outer = 10; let f = fn(x ~ int) -> int { x + outer }; f(5)"));
}

#[test]
fn regression_prevent_arithmetic_regressions() {
    // Comprehensive tests to prevent arithmetic regressions
    
    // Basic operations
    assert_eq!(5, interpret("2 + 3"));
    assert_eq!(-1, interpret("2 - 3"));
    assert_eq!(6, interpret("2 * 3"));
    assert_eq!(2, interpret("6 / 3"));
    
    // Precedence
    assert_eq!(14, interpret("2 + 3 * 4"));
    assert_eq!(20, interpret("(2 + 3) * 4"));
    assert_eq!(10, interpret("2 * 3 + 4"));
    assert_eq!(14, interpret("2 * (3 + 4)"));
    
    // Unary operations
    assert_eq!(-5, interpret("-5"));
    assert_eq!(5, interpret("-(-5)"));
    assert_eq!(-4, interpret("2 * -2"));
    assert_eq!(4, interpret("(-2) * (-2)"));
    
    // Complex expressions
    assert_eq!(40, interpret("((2 + 3) * 4) + (6 / 3) * 10"));  // (5 * 4) + (2 * 10) = 20 + 20 = 40
}

#[test]
fn regression_prevent_function_regressions() {
    // Comprehensive tests to prevent function-related regressions
    
    // Basic function definition and calling
    assert_eq!(8, interpret("let add = fn(x ~ int, y ~ int) -> int { x + y }; add(3, 5)"));
    assert_eq!(25, interpret("let square = fn(x ~ int) -> int { x * x }; square(5)"));
    
    // Functions with no parameters
    assert_eq!(42, interpret("let get42 = fn() -> int { 42 }; get42()"));
    
    // Functions with boolean parameters and returns
    assert_eq!(1, interpret("let not_fn = fn(x ~ bool) -> bool { false if x else true }; not_fn(false)"));
    
    // Nested function calls
    assert_eq!(14, interpret("let double = fn(x ~ int) -> int { x * 2 }; let add = fn(a ~ int, b ~ int) -> int { a + b }; add(double(3), double(4))"));
    
    // Functions with type annotations  
    assert_eq!(15, interpret("let multiply ~ fn(int, int) -> int = fn(x ~ int, y ~ int) -> int { x * y }; multiply(3, 5)"));
}

#[test]
fn regression_prevent_variable_regressions() {
    // Comprehensive tests to prevent variable-related regressions
    
    // Basic variable operations
    assert_eq!(5, interpret("let x = 5; x"));
    assert_eq!(10, interpret("let x = 5; x = 10; x"));
    assert_eq!(15, interpret("let x = 5; let y = 10; x + y"));
    
    // Variable scoping
    assert_eq!(5, interpret("{ let x = 5; x }"));
    assert_eq!(8, interpret("{ let x = 3; { let x = 5; x + 3 } }"));  // Shadowing
    
    // Variable with type annotations
    assert_eq!(100, interpret("let x ~ int = 100; x"));
    assert_eq!(1, interpret("let flag ~ bool = true; flag"));
}

#[test]
fn regression_prevent_conditional_regressions() {
    // Comprehensive tests to prevent conditional-related regressions
    
    // Basic conditionals
    assert_eq!(1, interpret("1 if true else 0"));
    assert_eq!(0, interpret("1 if false else 0"));
    
    // Conditionals with expressions
    assert_eq!(10, interpret("5 + 5 if true else 3 + 3"));
    assert_eq!(6, interpret("5 + 5 if false else 3 + 3"));
    
    // Nested conditionals
    assert_eq!(1, interpret("1 if true else 2 if true else 3"));
    assert_eq!(3, interpret("1 if false else 2 if false else 3"));
    
    // Conditional precedence
    assert_eq!(3, interpret("1 + 2 if true else 3 * 4"));
    assert_eq!(12, interpret("1 + 2 if false else 3 * 4"));
}

#[test]
fn regression_error_handling_stability() {
    // Ensure error handling remains stable and doesn't crash
    
    // Syntax errors
    assert!(expect_error("let x = ;"));
    assert!(expect_error("5 +"));
    assert!(expect_error("(5 + 3"));
    
    // Type errors
    assert!(expect_error("true + 5"));
    assert!(expect_error("let x ~ int = true;"));
    
    // Undefined variable errors
    assert!(expect_error("unknown_var"));
    assert!(expect_error("let x = unknown_var;"));
    
    // Function call errors
    assert!(expect_error("let x = 5; x();"));
    assert!(expect_error("let f = fn(x ~ int) -> int { x }; f();"));
}

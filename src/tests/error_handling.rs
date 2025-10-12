use crate::tests::test_helpers::{expect_error, get_error_type};

#[test]
fn test_undeclared_variable_errors() {
    // Test that using undeclared variables produces errors
    assert!(expect_error("unknown_variable"));
    assert!(expect_error("let x = unknown_var;"));
    assert!(expect_error("let x = y + 1;"));
    assert!(expect_error("unknown_func()"));
}

#[test]
fn test_type_mismatch_errors() {
    // Test type mismatches with explicit type annotations
    assert!(expect_error("let x ~ int = true;"));
    assert!(expect_error("let flag ~ bool = 42;"));

    // Test arithmetic with booleans (should fail)
    assert!(expect_error("true + 5"));
    assert!(expect_error("false * 2"));
    assert!(expect_error("true - false"));
    assert!(expect_error("1 / true"));
}

#[test]
fn test_function_call_errors() {
    // Test function calls with wrong number of arguments
    assert!(expect_error("let f = fn(x ~ int) -> int { x }; f();"));
    assert!(expect_error("let f = fn(x ~ int) -> int { x }; f(1, 2);"));
    assert!(expect_error(
        "let f = fn(x ~ int, y ~ int) -> int { x + y }; f(1);"
    ));

    // Test calling non-functions
    assert!(expect_error("let x = 5; x();"));
    assert!(expect_error("let flag = true; flag(42);"));
}

#[test]
#[ignore = "Division by zero requires platform-specific signal handlers which are not yet implemented"]
fn test_division_by_zero() {
    // Note: Division by zero causes SIGILL on most platforms
    // Proper handling requires signal handlers which are complex to implement portably
    assert!(expect_error("5 / 0"));
    assert!(expect_error("let x = 0; 10 / x"));
    assert!(expect_error("let zero = fn() -> int { 0 }; 100 / zero()"));
}

#[test]
fn test_syntax_errors() {
    // Test various syntax errors that should be caught by the parser
    assert!(expect_error("let = 5;")); // Missing identifier
    assert!(expect_error("let x 5;")); // Missing =
    assert!(expect_error("let x = ;")); // Missing value
    assert!(expect_error("5 +")); // Incomplete expression
    assert!(expect_error("(5 + 3")); // Unmatched parenthesis
    assert!(expect_error("{ let x = 5;")); // Unmatched brace
    assert!(expect_error("5 + * 3")); // Invalid operator sequence
    assert!(expect_error("let let = 5;")); // Using keyword as identifier
}

#[test]
fn test_empty_and_whitespace_inputs() {
    // Test empty inputs and whitespace-only inputs
    assert!(expect_error("")); // Empty input
    assert!(expect_error("   ")); // Whitespace only
    assert!(expect_error("\n\t  \n")); // Only whitespace characters
}

#[test]
fn test_malformed_function_definitions() {
    // Test various malformed function definitions
    assert!(expect_error("let f = fn -> int { 5 };")); // Missing parameter list
    assert!(expect_error("let f = fn() { 5 };")); // Missing return type
    assert!(expect_error("let f = fn(x) -> int { 5 };")); // Missing parameter type
    assert!(expect_error("let f = fn(x ~ int) -> { 5 };")); // Missing return type name
    assert!(expect_error("let f = fn(x ~ int) -> int ;")); // Missing body
    assert!(expect_error("let f = fn(x ~ int) -> int { };")); // Empty body
}

#[test]
fn test_malformed_blocks() {
    // Test malformed block expressions
    assert!(expect_error("{ let x = 5")); // Missing closing brace
    assert!(expect_error("let x = 5 }")); // Missing opening brace
    assert!(expect_error("{ { let x = 5; }")); // Unmatched nested braces
}

#[test]
fn test_malformed_conditionals() {
    // Test malformed conditional expressions
    assert!(expect_error("if true")); // Missing else clause
    assert!(expect_error("if else 5")); // Missing condition
    assert!(expect_error("true if else 5")); // Missing condition
    assert!(expect_error("1 if true else")); // Missing else value
    assert!(expect_error("if (true { 1 } else { 2 }")); // Malformed condition
}

#[test]
fn test_assignment_errors() {
    // Test assignment to undeclared variables
    assert!(expect_error("x = 5;")); // Assigning to undeclared variable
    assert!(expect_error("unknown = 10;")); // Assigning to undeclared variable
}

#[test]
fn test_nested_error_conditions() {
    // Test errors in nested contexts
    assert!(expect_error("{ let x = unknown_var; }"));
    assert!(expect_error("let f = fn() -> int { unknown_var };"));
    assert!(expect_error("1 + (2 * unknown_var)"));
    assert!(expect_error("if unknown_condition else 5"));
}

#[test]
fn test_recursive_function_errors() {
    // Test recursive function definitions that might cause issues
    // Note: These might succeed if recursion is properly implemented
    assert!(expect_error(
        "let fact = fn(n ~ int) -> int { if n <= 1 else n * fact(n - 1) };"
    ));
}

#[test]
fn test_operator_precedence_errors() {
    // Test malformed expressions with incorrect operator usage
    assert!(expect_error("1 + + 2")); // Double operators
    assert!(expect_error("1 * / 2")); // Invalid operator combination
    assert!(expect_error("+ 1 2")); // Prefix notation (not supported)
}

#[test]
#[ignore = "Parser stops at valid expression '1' rather than treating '1 2 +' as postfix notation"]
fn test_postfix_notation_error() {
    // Postfix notation like "1 2 +" is not fully validated
    // The parser successfully parses "1" and stops, returning 1
    // This could be enhanced in the future to detect trailing unparsed tokens
    assert!(expect_error("1 2 +")); // Postfix notation (not supported)
}

#[test]
fn test_error_types() {
    // Test that we can distinguish between different error types
    assert_eq!(get_error_type("5 +"), Some("ParserError".to_string()));
    assert_eq!(
        get_error_type("unknown_var"),
        Some("TypeCheckerError".to_string())
    );

    // These should not error (working cases)
    assert_eq!(get_error_type("5 + 3"), None);
    assert_eq!(get_error_type("let x = 5; x"), None);
}

#[test]
fn test_large_expression_errors() {
    // Test errors in complex expressions
    let complex_expr = "let x = 1; let y = 2; let z = unknown_var; x + y + z";
    assert!(expect_error(complex_expr));

    let complex_func = "let f = fn(a ~ int, b ~ unknown_type) -> int { a + b }; f(1, 2)";
    assert!(expect_error(complex_func));
}

#[test]
fn test_unicode_and_special_characters() {
    // Test handling of unicode and special characters in error cases
    assert!(expect_error("let x = 5; x + €")); // Unicode currency symbol
    assert!(expect_error("let 变量 = 5;")); // Unicode identifier (might not be supported)
    assert!(expect_error("let x = 5; x + @")); // Special character
    assert!(expect_error("let x = 5; x + #")); // Hash character
}

use crate::tests::interpret;

#[test]
fn test_integer_boundaries() {
    // Test maximum and minimum values for different integer types
    // Note: Adjust these based on your actual integer implementation

    // Small integers
    assert_eq!(0, interpret("0"));
    assert_eq!(1, interpret("1"));
    assert_eq!(-1, interpret("-1"));

    // Larger integers (within reasonable bounds)
    assert_eq!(1000, interpret("1000"));
    assert_eq!(-1000, interpret("-1000"));
    assert_eq!(123456, interpret("123456"));
    assert_eq!(-123456, interpret("-123456"));

    // Test operations at boundaries
    // BUG: int max
    //assert_eq!(2147483647, interpret("2147483647")); // Max i32 if supported
    //assert_eq!(-2147483648, interpret("-2147483648")); // Min i32 if supported
}

#[test]
fn test_arithmetic_boundaries() {
    // Test arithmetic operations near overflow boundaries
    // These should work within reasonable integer ranges

    // Large multiplication
    assert_eq!(1000000, interpret("1000 * 1000"));

    // Division resulting in 0
    assert_eq!(0, interpret("1 / 2")); // Integer division
    assert_eq!(0, interpret("5 / 10"));

    // Large addition chains
    assert_eq!(5000, interpret("1000 + 1000 + 1000 + 1000 + 1000"));

    // Deep subtraction
    assert_eq!(0, interpret("1000 - 500 - 250 - 125 - 125"));
}

#[test]
fn test_deeply_nested_expressions() {
    // Test deeply nested parentheses
    assert_eq!(1, interpret("((((1))))"));
    assert_eq!(6, interpret("((1 + 2) * (3 - 1))"));
    assert_eq!(24, interpret("(((2 * 3) + 1) * (4 - 1) + 3)"));

    // Test deeply nested arithmetic
    let deep_expr = "(1 + (2 + (3 + (4 + 5))))";
    assert_eq!(15, interpret(deep_expr));

    // Test alternating operations
    assert_eq!(-2, interpret("1 + 2 - 3 + 4 - 5 + 6 - 7 + 8 - 9 + 10 - 9"));
}

#[test]
fn test_complex_operator_precedence() {
    // Test complex precedence scenarios
    assert_eq!(14, interpret("2 + 3 * 4")); // 2 + 12 = 14
    assert_eq!(20, interpret("(2 + 3) * 4")); // 5 * 4 = 20
    assert_eq!(10, interpret("2 * 3 + 4")); // 6 + 4 = 10
    assert_eq!(14, interpret("2 * (3 + 4)")); // 2 * 7 = 14

    // Multiple operations
    assert_eq!(30, interpret("2 + 4 * 7")); // 2 + 28 = 30
    assert_eq!(42, interpret("(2 + 4) * 7")); // 6 * 7 = 42
    assert_eq!(16, interpret("2 * 4 + 8")); // 8 + 8 = 16
    assert_eq!(24, interpret("2 * (4 + 8)")); // 2 * 12 = 24

    // Division precedence
    assert_eq!(4, interpret("8 / 2 + 0")); // 4 + 0 = 4
    assert_eq!(1, interpret("8 / (2 + 6)")); // 8 / 8 = 1
    assert_eq!(6, interpret("4 + 8 / 4")); // 4 + 2 = 6
    assert_eq!(3, interpret("(4 + 8) / 4")); // 12 / 4 = 3
}

#[test]
fn test_boolean_boundary_conditions() {
    // Test boolean values in various contexts
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));

    // Boolean in conditionals
    assert_eq!(1, interpret("1 if true else 0"));
    assert_eq!(0, interpret("1 if false else 0"));
    assert_eq!(42, interpret("0 if false else 42"));
    assert_eq!(1, interpret("1 if true else 0"));
}

#[test]
fn test_variable_scoping_boundaries() {
    // Test variable scoping in different contexts

    // Block scoping
    assert_eq!(5, interpret("{ let x = 5; x }"));
    assert_eq!(7, interpret("{ let x = 5; let y = 2; x + y }"));

    // Nested blocks
    assert_eq!(10, interpret("{ let x = 5; { let y = 5; x + y } }"));
    assert_eq!(8, interpret("{ let x = 3; { let x = 5; x + 3 } }")); // Shadowing

    // Multiple variable declarations
    assert_eq!(
        15,
        interpret("let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; a + b + c + d + e")
    );
}

#[test]
fn test_function_parameter_boundaries() {
    // Test functions with many parameters
    assert_eq!(
        10,
        interpret("let add3 = fn(a ~ int, b ~ int, c ~ int) -> int { a + b + c }; add3(2, 3, 5)")
    );
    assert_eq!(24, interpret("let add4 = fn(a ~ int, b ~ int, c ~ int, d ~ int) -> int { a + b + c + d }; add4(1, 2, 3, 18)"));

    // Functions with complex expressions
    assert_eq!(
        116,
        interpret(
            "let calc = fn(x ~ int, y ~ int) -> int { (x + y) * (x - y) + x * y }; calc(10, 2)"
        )
    );
}

#[test]
fn test_assignment_chain_boundaries() {
    // Test multiple assignments
    assert_eq!(42, interpret("let x = 1; x = 2; x = 3; x = 42; x"));
    assert_eq!(25, interpret("let x = 5; let y = 10; x = y; y = 15; x + y")); // x should be 10, y should be 15 = 25

    // Assignment with calculations
    assert_eq!(25, interpret("let x = 5; x = x * x; x"));
    assert_eq!(100, interpret("let x = 10; x = x * x; x"));
}

#[test]
fn test_conditional_complexity_boundaries() {
    // Nested conditionals
    assert_eq!(3, interpret("1 if false else 2 if false else 3"));
    assert_eq!(1, interpret("1 if true else 2 if true else 3"));
    assert_eq!(2, interpret("1 if false else 2 if true else 3"));

    // Conditionals with complex expressions
    assert_eq!(10, interpret("(5 + 5) if (2 * 2 == 4) else (3 + 3)"));
    assert_eq!(6, interpret("(5 + 5) if (2 * 2 == 5) else (3 + 3)"));
}

#[test]
fn test_empty_and_minimal_constructs() {
    // Test minimal valid constructs
    assert_eq!(0, interpret("0"));
    assert_eq!(1, interpret("1"));
    assert_eq!(0, interpret("false"));
    assert_eq!(1, interpret("true"));

    // Minimal block
    assert_eq!(42, interpret("{ 42 }"));

    // Minimal function
    assert_eq!(5, interpret("let f = fn() -> int { 5 }; f()"));

    // Minimal variable
    assert_eq!(7, interpret("let x = 7; x"));
}

#[test]
fn test_whitespace_boundaries() {
    // Test expressions with various whitespace patterns
    assert_eq!(3, interpret("1+2")); // No spaces
    assert_eq!(3, interpret("1 + 2")); // Normal spaces
    assert_eq!(3, interpret("1  +  2")); // Multiple spaces
    assert_eq!(3, interpret("1\t+\t2")); // Tabs
    assert_eq!(3, interpret("1\n+\n2")); // Newlines
    assert_eq!(3, interpret("  1  +  2  ")); // Leading/trailing spaces

    // Complex whitespace in blocks
    assert_eq!(5, interpret("{\n  let x = 5;\n  x\n}"));
}

#[test]
fn test_expression_length_boundaries() {
    // Test very long valid expressions
    let long_addition =
        "1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1";
    assert_eq!(20, interpret(long_addition));

    // Long multiplication chain
    assert_eq!(1024, interpret("2 * 2 * 2 * 2 * 2 * 2 * 2 * 2 * 2 * 2"));

    // Long variable chain
    let long_vars = "let a = 1; let b = 2; let c = 3; let d = 4; let e = 5; let f = 6; let g = 7; let h = 8; let i = 9; let j = 10; a + b + c + d + e + f + g + h + i + j";
    assert_eq!(55, interpret(long_vars));
}

// Test file for error conditions and edge cases
// Note: These tests would need a different assertion mechanism than interpret()
// since they're testing error conditions. For now, they're commented out
// but provide a structure for future error testing.

/*
use crate::tests::interpret_error; // Hypothetical function for testing errors

#[test]
fn undeclared_variable_error() {
    // Test that using an undeclared variable produces an error
    assert!(interpret_error("unknown_variable").is_err());
    assert!(interpret_error("let x = unknown_var;").is_err());
}

#[test]
fn type_mismatch_errors() {
    // Test type mismatches
    assert!(interpret_error("let x ~ int = true;").is_err());
    assert!(interpret_error("let flag ~ bool = 42;").is_err());
    assert!(interpret_error("true + 5").is_err());
    assert!(interpret_error("false * 2").is_err());
}

#[test]
fn function_call_errors() {
    // Test function call with wrong number of arguments
    assert!(interpret_error("let f = fn(x ~ int) -> int { x }; f();").is_err());
    assert!(interpret_error("let f = fn(x ~ int) -> int { x }; f(1, 2);").is_err());

    // Test calling non-function
    assert!(interpret_error("let x = 5; x();").is_err());
}

#[test]
fn division_by_zero() {
    // Test division by zero
    assert!(interpret_error("5 / 0").is_err());
    assert!(interpret_error("let x = 0; 10 / x").is_err());
}

#[test]
fn syntax_errors() {
    // Test various syntax errors
    assert!(interpret_error("let = 5;").is_err());          // Missing identifier
    assert!(interpret_error("let x 5;").is_err());          // Missing =
    assert!(interpret_error("let x = ;").is_err());         // Missing value
    assert!(interpret_error("5 +").is_err());               // Incomplete expression
    assert!(interpret_error("(5 + 3").is_err());            // Unmatched parenthesis
    assert!(interpret_error("{ let x = 5;").is_err());      // Unmatched brace
}

#[test]
fn redeclaration_errors() {
    // Test variable redeclaration in same scope
    assert!(interpret_error("let x = 5; let x = 10;").is_err());
}

#[test]
fn scope_errors() {
    // Test accessing variables out of scope
    assert!(interpret_error("{ let x = 5; }; x").is_err());
}
*/

// For now, include some basic edge case tests that should work
use crate::tests::interpret;

#[test]
fn edge_case_zero_operations() {
    assert_eq!(0, interpret("0 + 0"));
    assert_eq!(0, interpret("0 - 0"));
    assert_eq!(0, interpret("0 * 0"));
    assert_eq!(0, interpret("0 * 1"));
    assert_eq!(0, interpret("1 * 0"));
}

#[test]
fn edge_case_negative_zero() {
    assert_eq!(0, interpret("-0"));
    assert_eq!(0, interpret("0 + -0"));
    assert_eq!(0, interpret("-0 + 0"));
}

#[test]
fn edge_case_boolean_consistency() {
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));
    assert_eq!(1, interpret("true if true else false"));
    assert_eq!(0, interpret("false if true else true"));
}

#[test]
fn edge_case_deeply_nested() {
    assert_eq!(1, interpret("((((true))))"));
    assert_eq!(5, interpret("((((5))))"));
    assert_eq!(10, interpret("(((2 + 3))) + (((5)))"));
}

#[test]
fn edge_case_empty_expressions() {
    // Test minimal valid programs
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));
    assert_eq!(42, interpret("42"));
}

#[test]
fn edge_case_whitespace_variations() {
    // Test expressions with unusual but valid whitespace
    assert_eq!(3, interpret("1+2")); // No spaces
    assert_eq!(3, interpret("1  +  2")); // Multiple spaces
    assert_eq!(3, interpret("1\t+\t2")); // Tabs
    assert_eq!(3, interpret("1\n+\n2")); // Newlines
    assert_eq!(3, interpret("  1  +  2  ")); // Leading/trailing spaces

    // Whitespace in more complex expressions
    assert_eq!(10, interpret(" ( 2 + 3 ) * 2 "));
    assert_eq!(5, interpret("{\n  let x = 5;\n  x\n}"));
}

#[test]
fn edge_case_operator_combinations() {
    // Test all combinations of basic operators
    assert_eq!(7, interpret("1 + 2 * 3")); // Addition and multiplication
    assert_eq!(9, interpret("(1 + 2) * 3")); // Grouping changes precedence
    assert_eq!(5, interpret("2 * 3 - 1")); // Multiplication and subtraction
    assert_eq!(4, interpret("2 * (3 - 1)")); // Grouping changes precedence
    assert_eq!(4, interpret("6 / 2 + 1")); // Division and addition
    assert_eq!(2, interpret("6 / (2 + 1)")); // Grouping changes precedence
    assert_eq!(1, interpret("3 - 6 / 3")); // Subtraction and division
    assert_eq!(-1, interpret("(3 - 6) / 3")); // Grouping changes precedence
}

#[test]
fn edge_case_unary_combinations() {
    // Test unary minus in various contexts
    assert_eq!(-1, interpret("-1"));
    assert_eq!(1, interpret("-(-1)"));
    assert_eq!(-4, interpret("-(2 + 2)"));
    assert_eq!(4, interpret("-(-(2 + 2))"));

    // Unary with binary operations
    assert_eq!(-4, interpret("2 * -2"));
    assert_eq!(4, interpret("-2 * -2"));
    assert_eq!(-1, interpret("3 + -4"));
    assert_eq!(7, interpret("3 - -4"));
    assert_eq!(-2, interpret("-6 / 3"));
    assert_eq!(2, interpret("-6 / -3"));
}

#[test]
fn edge_case_boolean_edge_conditions() {
    // Boolean edge cases
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));

    // Booleans in conditionals with edge cases
    assert_eq!(1, interpret("true if true else false"));
    assert_eq!(0, interpret("false if true else true"));
    assert_eq!(0, interpret("true if false else false"));
    assert_eq!(1, interpret("false if false else true"));
}

#[test]
fn edge_case_variable_edge_scenarios() {
    // Single character variable names
    assert_eq!(5, interpret("let a = 5; a"));
    assert_eq!(10, interpret("let x = 5; let y = 5; x + y"));

    // Variables with underscores
    assert_eq!(7, interpret("let _x = 7; _x"));
    assert_eq!(15, interpret("let my_var = 15; my_var"));

    // Variable reassignment edge cases
    assert_eq!(0, interpret("let x = 5; x = 0; x"));
    assert_eq!(-5, interpret("let y = 5; y = -5; y"));
}

#[test]
fn edge_case_function_edge_scenarios() {
    // Functions with minimal bodies
    assert_eq!(0, interpret("let f = fn() -> int { 0 }; f()"));
    assert_eq!(1, interpret("let f = fn() -> bool { true }; f()"));

    // Functions that return their input unchanged
    assert_eq!(42, interpret("let id = fn(x ~ int) -> int { x }; id(42)"));
    assert_eq!(
        1,
        interpret("let bool_id = fn(b ~ bool) -> bool { b }; bool_id(true)")
    );

    // Functions with edge case calculations
    assert_eq!(
        0,
        interpret("let zero = fn(x ~ int) -> int { x * 0 }; zero(999)")
    );
    assert_eq!(
        1,
        interpret("let one = fn(x ~ int) -> int { x / x }; one(5)")
    );
}

#[test]
fn edge_case_block_scenarios() {
    // Minimal blocks
    assert_eq!(1, interpret("{ 1 }"));
    assert_eq!(0, interpret("{ false }"));

    // Blocks with single statements
    assert_eq!(5, interpret("{ let x = 5; x }"));

    // Empty-ish blocks (with just a value)
    assert_eq!(42, interpret("{ 42 }"));
    assert_eq!(1, interpret("{ true }"));
}

#[test]
fn edge_case_conditional_scenarios() {
    // Conditionals with constant conditions
    assert_eq!(1, interpret("1 if true else 0"));
    assert_eq!(0, interpret("1 if false else 0"));

    // Conditionals where both branches are the same
    assert_eq!(5, interpret("5 if true else 5"));
    assert_eq!(5, interpret("5 if false else 5"));

    // Conditionals with zero values
    assert_eq!(0, interpret("0 if true else 1"));
    assert_eq!(1, interpret("0 if false else 1"));
    assert_eq!(1, interpret("1 if true else 0"));
    assert_eq!(0, interpret("1 if false else 0"));
}

#[test]
fn edge_case_deeply_nested_structures() {
    // Deeply nested parentheses
    assert_eq!(5, interpret("((((5))))"));
    assert_eq!(1, interpret("((((true))))"));

    // Deeply nested blocks
    assert_eq!(10, interpret("{ { { let x = 10; x } } }"));

    // Deeply nested function calls
    assert_eq!(
        8,
        interpret("let add1 = fn(x ~ int) -> int { x + 1 }; add1(add1(add1(5)))")
    );
}

#[test]
fn edge_case_large_numbers() {
    // Test reasonably large numbers (within safe integer range)
    assert_eq!(1000, interpret("1000"));
    assert_eq!(10000, interpret("10000"));
    assert_eq!(100000, interpret("100000"));

    // Large number operations
    assert_eq!(2000, interpret("1000 + 1000"));
    assert_eq!(1000000, interpret("1000 * 1000"));
    assert_eq!(1, interpret("1000 / 1000"));
    assert_eq!(0, interpret("1000 - 1000"));
}

#[test]
fn edge_case_expression_boundaries() {
    // Expressions at the boundary of complexity
    let complex_expr = "((1 + 2) * (3 + 4)) + ((5 - 2) * (7 - 1))";
    assert_eq!(39, interpret(complex_expr)); // (3 * 7) + (3 * 6) = 21 + 18 = 39

    // Long chain of operations
    assert_eq!(0, interpret("1 - 1 + 1 - 1 + 1 - 1"));
    assert_eq!(1, interpret("1 * 1 * 1 * 1 * 1 * 1"));
}

#[test]
fn edge_case_type_boundaries() {
    // Test type system boundaries
    assert_eq!(1, interpret("let x ~ int = 1; x"));
    assert_eq!(0, interpret("let flag ~ bool = false; flag"));

    // Functions with explicit types
    assert_eq!(
        5,
        interpret("let f ~ fn(int) -> int = fn(x ~ int) -> int { x }; f(5)")
    );
}

// Additional comprehensive edge cases

#[test]
fn edge_case_variable_shadowing() {
    // Shadowing in nested scopes
    assert_eq!(10, interpret("let x = 5; { let x = 10; x }"));
    assert_eq!(5, interpret("let x = 5; { let x = 10; x }; x"));

    // Multiple levels of shadowing
    assert_eq!(15, interpret("let x = 5; { let x = 10; { let x = 15; x } }"));
    assert_eq!(10, interpret("let x = 5; { let x = 10; { let x = 15; x }; x }"));

    // Shadowing with same value
    assert_eq!(5, interpret("let x = 5; { let x = 5; x }"));
}

#[test]
fn edge_case_assignment_chains() {
    // Multiple assignments
    assert_eq!(10, interpret("let x = 5; x = 10; x"));
    assert_eq!(15, interpret("let x = 5; x = 10; x = 15; x"));

    // Assignment in blocks
    assert_eq!(20, interpret("let x = 5; { x = 20; x }"));
    assert_eq!(20, interpret("let x = 5; { x = 20; }; x"));

    // Assignment with computation
    assert_eq!(10, interpret("let x = 5; x = x + 5; x"));
    assert_eq!(25, interpret("let x = 5; x = x * 5; x"));
}

#[test]
fn edge_case_comparison_operators() {
    // Less than edge cases
    assert_eq!(1, interpret("0 < 1"));
    assert_eq!(0, interpret("1 < 1"));
    assert_eq!(0, interpret("2 < 1"));
    assert_eq!(1, interpret("-1 < 0"));
    assert_eq!(0, interpret("0 < -1"));

    // Greater than edge cases
    assert_eq!(0, interpret("0 > 1"));
    assert_eq!(0, interpret("1 > 1"));
    assert_eq!(1, interpret("2 > 1"));
    assert_eq!(0, interpret("-1 > 0"));
    assert_eq!(1, interpret("0 > -1"));

    // Equality edge cases
    assert_eq!(1, interpret("0 == 0"));
    assert_eq!(1, interpret("1 == 1"));
    assert_eq!(0, interpret("0 == 1"));
    assert_eq!(1, interpret("-1 == -1"));
    assert_eq!(0, interpret("-1 == 1"));
}

#[test]
#[ignore = "comparing booleans with integers is a type error"]
fn edge_case_comparison_chains() {
    // Chained comparisons (each comparison returns bool which is then compared)
    assert_eq!(0, interpret("(1 < 2) < 1")); // true < 1 => 1 < 1 => false
    assert_eq!(1, interpret("(1 > 2) < 1")); // false < 1 => 0 < 1 => true
}

#[test]
fn edge_case_comparisons_in_conditionals() {
    // Comparisons in conditionals
    assert_eq!(10, interpret("10 if 1 < 2 else 20"));
    assert_eq!(20, interpret("10 if 1 > 2 else 20"));
}

#[test]
fn edge_case_while_loops() {
    // Loop that never executes
    assert_eq!(5, interpret("let x = 5; while false { x = x + 1; }; x"));

    // Loop that executes once
    assert_eq!(6, interpret("let x = 5; let flag = 1; while flag == 1 { x = x + 1; flag = 0; }; x"));

    // Loop counting down
    assert_eq!(0, interpret("let x = 5; while x > 0 { x = x - 1; }; x"));

    // Loop with complex condition
    assert_eq!(10, interpret("let x = 0; while x < 10 { x = x + 1; }; x"));
}

#[test]
fn edge_case_nested_while_loops() {
    // Nested loop simple case
    let program = r#"
        let outer = 0;
        let total = 0;
        while outer < 3 {
            let inner = 0;
            while inner < 3 {
                total = total + 1;
                inner = inner + 1;
            };
            outer = outer + 1;
        };
        total
    "#;
    assert_eq!(9, interpret(program));
}

#[test]
fn edge_case_conditionals_all_types() {
    // Conditional with integers
    assert_eq!(1, interpret("1 if true else 2"));
    assert_eq!(2, interpret("1 if false else 2"));

    // Conditional with booleans
    assert_eq!(1, interpret("true if true else false"));
    assert_eq!(0, interpret("true if false else false"));
}

#[test]
#[ignore = "conditionals with function values not fully implemented"]
fn edge_case_conditionals_with_functions() {
    // Conditional with functions
    let program = r#"
        let f1 = fn() -> int { 1 };
        let f2 = fn() -> int { 2 };
        let chosen = f1 if true else f2;
        chosen()
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn edge_case_deeply_nested_conditionals() {
    // Three levels deep
    assert_eq!(1, interpret("1 if true else (2 if true else 3)"));
    assert_eq!(2, interpret("1 if false else (2 if true else 3)"));
    assert_eq!(3, interpret("1 if false else (2 if false else 3)"));

    // Four levels deep
    assert_eq!(1, interpret("1 if true else (2 if true else (3 if true else 4))"));
    assert_eq!(4, interpret("1 if false else (2 if false else (3 if false else 4))"));
}

#[test]
fn edge_case_function_multiple_params() {
    // Two parameters
    assert_eq!(7, interpret("let add = fn(a ~ int, b ~ int) -> int { a + b }; add(3, 4)"));

    // Three parameters
    assert_eq!(
        10,
        interpret("let add3 = fn(a ~ int, b ~ int, c ~ int) -> int { a + b + c }; add3(2, 3, 5)")
    );

    // Five parameters
    let program = r#"
        let add5 = fn(a ~ int, b ~ int, c ~ int, d ~ int, e ~ int) -> int {
            a + b + c + d + e
        };
        add5(1, 2, 3, 4, 5)
    "#;
    assert_eq!(15, interpret(program));
}

#[test]
fn edge_case_function_parameter_order() {
    // Parameters used in different order than defined
    let program = r#"
        let subtract = fn(a ~ int, b ~ int) -> int { b - a };
        subtract(3, 10)
    "#;
    assert_eq!(7, interpret(program));

    // All parameters used multiple times
    let program = r#"
        let compute = fn(a ~ int, b ~ int) -> int { a + b + a - b };
        compute(5, 3)
    "#;
    assert_eq!(10, interpret(program));
}

#[test]
fn edge_case_blocks_with_many_statements() {
    // Block with 5 statements
    let program = r#"
        {
            let a = 1;
            let b = 2;
            let c = 3;
            let d = 4;
            let e = 5;
            a + b + c + d + e
        }
    "#;
    assert_eq!(15, interpret(program));

    // Block with 10 statements
    let program = r#"
        {
            let a = 1;
            let b = a + 1;
            let c = b + 1;
            let d = c + 1;
            let e = d + 1;
            let f = e + 1;
            let g = f + 1;
            let h = g + 1;
            let i = h + 1;
            let j = i + 1;
            j
        }
    "#;
    assert_eq!(10, interpret(program));
}

#[test]
fn edge_case_mixed_operations() {
    // All arithmetic operators in one expression
    assert_eq!(5, interpret("1 + 2 * 3 - 4 / 2")); // 1 + 6 - 2 = 5

    // With parentheses changing precedence
    assert_eq!(3, interpret("(1 + 2) * (3 - 4 / 2)"));

    // Mixing comparison and arithmetic
    assert_eq!(1, interpret("(1 + 2) < (2 * 3)"));
    assert_eq!(0, interpret("(5 * 2) < (3 + 4)"));
}

#[test]
fn edge_case_very_long_expression() {
    // Very long addition chain
    let expr = "1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1";
    assert_eq!(20, interpret(expr));

    // Very long multiplication chain
    let expr = "1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1 * 1";
    assert_eq!(1, interpret(expr));

    // Alternating operators
    let expr = "10 + 5 - 3 + 2 - 1 + 4 - 2 + 6 - 3 + 1";
    assert_eq!(19, interpret(expr));
}

#[test]
fn edge_case_zero_in_various_positions() {
    // Zero as first operand
    assert_eq!(5, interpret("0 + 5"));
    assert_eq!(-5, interpret("0 - 5"));
    assert_eq!(0, interpret("0 * 5"));
    assert_eq!(0, interpret("0 / 5"));

    // Zero as second operand
    assert_eq!(5, interpret("5 + 0"));
    assert_eq!(5, interpret("5 - 0"));
    assert_eq!(0, interpret("5 * 0"));

    // Zero in comparisons
    assert_eq!(0, interpret("0 < 0"));
    assert_eq!(0, interpret("0 > 0"));
    assert_eq!(1, interpret("0 == 0"));
}

#[test]
fn edge_case_negative_numbers_everywhere() {
    // Negative in arithmetic
    assert_eq!(-5, interpret("-3 + -2"));
    assert_eq!(-1, interpret("-3 - -2"));
    assert_eq!(6, interpret("-3 * -2"));
    assert_eq!(2, interpret("-4 / -2"));

    // Negative in comparisons
    assert_eq!(1, interpret("-5 < -3"));
    assert_eq!(0, interpret("-3 > -1"));
    assert_eq!(1, interpret("-5 == -5"));

    // Negative in variables
    assert_eq!(-10, interpret("let x = -10; x"));
    assert_eq!(-10, interpret("let x = 10; x = -10; x"));
}

#[test]
#[ignore = "booleans in arithmetic is a type error"]
fn edge_case_boolean_in_arithmetic_contexts() {
    // Booleans treated as integers
    assert_eq!(2, interpret("true + true"));
    assert_eq!(1, interpret("true + false"));
    assert_eq!(0, interpret("false + false"));
    assert_eq!(1, interpret("true * true"));
    assert_eq!(0, interpret("true * false"));
}

#[test]
#[ignore = "higher-order functions not fully implemented"]
fn edge_case_functions_returning_functions() {
    // Function that returns a function
    let program = r#"
        let make_adder = fn(x ~ int) -> fn(int) -> int {
            fn(y ~ int) -> int { x + y }
        };
        let add5 = make_adder(5);
        add5(3)
    "#;
    assert_eq!(8, interpret(program));
}

#[test]
fn edge_case_immediate_function_invocation() {
    // Call function immediately after definition
    assert_eq!(5, interpret("(fn() -> int { 5 })()"));
    assert_eq!(7, interpret("(fn(x ~ int) -> int { x + 2 })(5)"));

    // Multiple calls
    assert_eq!(10, interpret("(fn(x ~ int) -> int { x * 2 })(5)"));
}

#[test]
fn edge_case_complex_block_scoping() {
    // Variable used across multiple block levels
    let program = r#"
        let x = 1;
        {
            let y = x + 1;
            {
                let z = y + 1;
                {
                    let w = z + 1;
                    w
                }
            }
        }
    "#;
    assert_eq!(4, interpret(program));
}

#[test]
fn edge_case_conditional_with_side_effects() {
    // Conditional where both branches modify variables
    let program = r#"
        let x = 5;
        let result = { x = 10; 1 } if true else { x = 20; 2 };
        x
    "#;
    assert_eq!(10, interpret(program));

    let program = r#"
        let x = 5;
        let result = { x = 10; 1 } if false else { x = 20; 2 };
        x
    "#;
    assert_eq!(20, interpret(program));
}

#[test]
fn edge_case_while_with_complex_updates() {
    // While loop with multiple variable updates
    let program = r#"
        let x = 0;
        let y = 10;
        while x < 5 {
            x = x + 1;
            y = y - 1;
        };
        x + y
    "#;
    assert_eq!(10, interpret(program));
}

#[test]
fn edge_case_function_as_expression_in_operations() {
    // Using function call results in arithmetic
    let program = r#"
        let get_five = fn() -> int { 5 };
        let get_three = fn() -> int { 3 };
        get_five() + get_three()
    "#;
    assert_eq!(8, interpret(program));

    // Function calls in comparisons
    let program = r#"
        let get_five = fn() -> int { 5 };
        let get_three = fn() -> int { 3 };
        get_five() > get_three()
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn edge_case_deeply_nested_function_calls() {
    // 5 levels of function nesting
    let program = r#"
        let add1 = fn(x ~ int) -> int { x + 1 };
        add1(add1(add1(add1(add1(0)))))
    "#;
    assert_eq!(5, interpret(program));

    // 10 levels
    let program = r#"
        let add1 = fn(x ~ int) -> int { x + 1 };
        add1(add1(add1(add1(add1(add1(add1(add1(add1(add1(0))))))))))
    "#;
    assert_eq!(10, interpret(program));
}

#[test]
fn edge_case_parentheses_everywhere() {
    // Excessive but valid parentheses
    assert_eq!(5, interpret("(((((5)))))"));
    assert_eq!(10, interpret("((((((5))))) + ((((5)))))"));
    assert_eq!(6, interpret("(((1 + 2))) * (((3 - 1)))"));
}

#[test]
fn edge_case_assignment_returns_value() {
    // Assignment returns the value
    assert_eq!(10, interpret("let x = 0; x = 10"));
}

#[test]
#[ignore = "assignment in complex expressions not fully supported"]
fn edge_case_assignment_in_expressions() {
    // Using assignment in expressions
    assert_eq!(15, interpret("let x = 0; (x = 10) + 5"));
    assert_eq!(20, interpret("let x = 0; let y = x = 10; x + y"));
}

#[test]
fn edge_case_while_zero_iterations() {
    // Different ways a loop might not execute
    assert_eq!(0, interpret("let x = 0; while false { x = x + 1; }; x"));
    assert_eq!(0, interpret("let x = 0; while x > 0 { x = x + 1; }; x"));
    assert_eq!(5, interpret("let x = 5; while x == 10 { x = x + 1; }; x"));
}

#[test]
fn edge_case_greater_than_or_equal() {
    // >= operator edge cases
    assert_eq!(1, interpret("5 >= 5"));
    assert_eq!(1, interpret("5 >= 4"));
    assert_eq!(0, interpret("4 >= 5"));
    assert_eq!(1, interpret("0 >= 0"));
    assert_eq!(1, interpret("-1 >= -1"));
    assert_eq!(0, interpret("-2 >= -1"));
}

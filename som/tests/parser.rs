mod common;

use common::test_parse;

#[test]
fn test_parse_simple_function() {
    let (ast, errors) = test_parse("fn add(x: i32, y: i32) -> i32 { x + y }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 1);
}

#[test]
fn test_parse_function_with_let() {
    let (_, errors) = test_parse("fn test() { let x: i32 = 5; x }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_binary_expr() {
    let (_, errors) = test_parse("fn test() { 1 + 2 * 3 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_function_call() {
    let (ast, errors) =
        test_parse("fn add(a: i32, b: i32) -> i32 { a + b } fn test() { add(1, 2) }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 2);
}

#[test]
fn test_parse_conditional_basic() {
    let (_, errors) = test_parse("fn test() -> i32 { 1 if true else 2 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_with_expressions() {
    let (_, errors) = test_parse("fn test() -> i32 { 1 + 2 if true else 3 + 4 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_nested() {
    let (_, errors) = test_parse("fn test() -> i32 { 1 if true else (2 if false else 3) }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_in_let() {
    let (_, errors) = test_parse("fn test() { let x = 1 if true else 2; }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_with_comparison() {
    let (_, errors) = test_parse("fn test(x: i32) -> i32 { 1 if x > 0 else 2 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_addition() {
    // Should parse as (1 + 2) if true else (3 + 4), not 1 + (2 if true else 3) + 4
    let (_, errors) = test_parse("fn test() -> i32 { 1 + 2 if true else 3 + 4 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_multiply() {
    // Should parse as (2 * 3) if true else (4 * 5)
    let (_, errors) = test_parse("fn test() -> i32 { 2 * 3 if true else 4 * 5 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_mixed() {
    // Should parse as (1 + 2 * 3) if (x > 0) else (4 - 5)
    let (_, errors) = test_parse("fn test(x: i32) -> i32 { 1 + 2 * 3 if x > 0 else 4 - 5 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_chained() {
    // a if b else c if d else e should parse as a if b else (c if d else e)
    let (_, errors) = test_parse("fn test(x: bool, y: bool) -> i32 { 1 if x else 2 if y else 3 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

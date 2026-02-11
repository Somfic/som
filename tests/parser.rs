use som::parser;
use som::Source;
use std::sync::Arc;

#[test]
fn test_parse_simple_function() {
    let source = Arc::new(Source::from_raw("fn add(x: i32, y: i32) -> i32 { x + y }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 1);
}

#[test]
fn test_parse_function_with_let() {
    let source = Arc::new(Source::from_raw("fn test() { let x: i32 = 5; x }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_binary_expr() {
    let source = Arc::new(Source::from_raw("fn test() { 1 + 2 * 3 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_function_call() {
    // Define add before test, so it's available for name resolution
    let source = Arc::new(Source::from_raw(
        "fn add(a: i32, b: i32) -> i32 { a + b } fn test() { add(1, 2) }",
    ));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 2);
}

#[test]
fn test_parse_conditional_basic() {
    let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 if true else 2 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_with_expressions() {
    let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 + 2 if true else 3 + 4 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_nested() {
    let source = Arc::new(Source::from_raw(
        "fn test() -> i32 { 1 if true else (2 if false else 3) }",
    ));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_in_let() {
    let source = Arc::new(Source::from_raw("fn test() { let x = 1 if true else 2; }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_with_comparison() {
    let source = Arc::new(Source::from_raw("fn test(x: i32) -> i32 { 1 if x > 0 else 2 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_addition() {
    // Should parse as (1 + 2) if true else (3 + 4), not 1 + (2 if true else 3) + 4
    let source = Arc::new(Source::from_raw("fn test() -> i32 { 1 + 2 if true else 3 + 4 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_multiply() {
    // Should parse as (2 * 3) if true else (4 * 5)
    let source = Arc::new(Source::from_raw("fn test() -> i32 { 2 * 3 if true else 4 * 5 }"));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_precedence_mixed() {
    // Should parse as (1 + 2 * 3) if (x > 0) else (4 - 5)
    let source = Arc::new(Source::from_raw(
        "fn test(x: i32) -> i32 { 1 + 2 * 3 if x > 0 else 4 - 5 }",
    ));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_conditional_chained() {
    // a if b else c if d else e should parse as a if b else (c if d else e)
    let source = Arc::new(Source::from_raw(
        "fn test(x: bool, y: bool) -> i32 { 1 if x else 2 if y else 3 }",
    ));
    let (ast, errors) = parser::parse(source);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

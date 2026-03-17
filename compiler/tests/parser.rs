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

#[test]
fn test_parse_while_statement() {
    let (_, errors) =
        test_parse("fn main() { let mut x = 0; while x < 10 { x = x + 1; } }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_struct_definition() {
    let (ast, errors) = test_parse("struct Vec2 { x: i32, y: i32 } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.structs.len(), 1);
}

#[test]
fn test_parse_impl_block() {
    let (ast, errors) = test_parse(
        "struct S { v: i32 } impl S { fn get(self: S) -> i32 { self.v } } fn main() {}",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert!(
        ast.func_registry.contains_key("S::get"),
        "func_registry should contain S::get"
    );
}

#[test]
fn test_parse_extern_block() {
    let (_, errors) = test_parse("extern { fn abs(x: i32) -> i32; } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_use_statement() {
    let (_, errors) = test_parse("use std; fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_block_expression() {
    let (_, errors) = test_parse("fn main() -> i32 { let x = { 1 + 2 }; x }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_nested_conditionals() {
    let (_, errors) = test_parse(
        "fn test(a: bool, b: bool, c: bool) -> i32 { 1 if a else (2 if b else (3 if c else 4)) }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_multiple_functions() {
    let (ast, errors) = test_parse(
        "fn foo() -> i32 { 1 } fn bar() -> i32 { 2 } fn baz() -> i32 { 3 }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 3);
}

#[test]
fn test_parse_missing_closing_brace() {
    let (_, errors) = test_parse("fn main() { 1");
    assert!(!errors.is_empty(), "Expected parse errors for missing closing brace");
}

#[test]
fn test_parse_complex_arithmetic() {
    let (_, errors) = test_parse("fn main() -> i32 { 1 + 2 * 3 - 4 / 2 + 5 % 3 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_modulo_expression() {
    let (_, errors) = test_parse("fn main() -> i32 { 10 % 3 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_let_mut() {
    let (_, errors) = test_parse("fn main() { let mut x = 0; x = 1; }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_struct_literal() {
    let (_, errors) = test_parse("struct P { x: i32 } fn main() { let p = P { x: 1 }; }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_field_access() {
    let (_, errors) =
        test_parse("struct P { x: i32 } fn main() -> i32 { let p = P { x: 1 }; p.x }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_method_call_with_args() {
    let (_, errors) = test_parse(
        "struct P { x: i32 } impl P { fn add(self: P, n: i32) -> i32 { self.x + n } } fn main() -> i32 { let p = P { x: 1 }; p.add(2) }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_static_method_call() {
    let (_, errors) = test_parse(
        "struct P { x: i32 } impl P { fn new(x: i32) -> P { P { x: x } } } fn main() { let p = P::new(1); }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_if_else_statement() {
    let (_, errors) =
        test_parse("fn main() { let mut x = 0; if true { x = 1; } else { x = 2; } }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_if_else_chain() {
    let (_, errors) = test_parse(
        "fn main() { let mut x = 0; if false { x = 1; } else if true { x = 2; } else { x = 3; } }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_nested_while() {
    let (_, errors) = test_parse(
        "fn main() { let mut i = 0; while i < 3 { let mut j = 0; while j < 3 { j = j + 1; } i = i + 1; } }",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_function_with_multiple_params() {
    let (ast, errors) =
        test_parse("fn add3(a: i32, b: i32, c: i32) -> i32 { a + b + c } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 2);
}

#[test]
fn test_parse_empty_function() {
    let (_, errors) = test_parse("fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_void_function() {
    let (_, errors) = test_parse("fn nothing() {} fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_multiple_let_bindings() {
    let (_, errors) = test_parse("fn main() { let a = 1; let b = 2; let c = 3; }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_comparison_operators() {
    let sources = [
        "fn main() -> bool { 1 < 2 }",
        "fn main() -> bool { 1 > 2 }",
        "fn main() -> bool { 1 <= 2 }",
        "fn main() -> bool { 1 >= 2 }",
        "fn main() -> bool { 1 == 2 }",
        "fn main() -> bool { 1 != 2 }",
    ];
    for source in &sources {
        let (_, errors) = test_parse(source);
        assert!(errors.is_empty(), "Errors for '{}': {:?}", source, errors);
    }
}

#[test]
fn test_parse_string_literal() {
    let (_, errors) = test_parse(r#"fn main() { let s = "hello"; }"#);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_bool_literal() {
    let (_, errors) = test_parse("fn main() { let b = true; let c = false; }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_reference_types() {
    let (_, errors) = test_parse("fn test(x: &i32) -> &i32 { x }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_mut_reference() {
    let (_, errors) = test_parse("fn test(x: &mut i32) {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_generic_function() {
    let (_, errors) = test_parse("fn identity<T>(x: T) -> T { x } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

#[test]
fn test_parse_return_struct() {
    let (_, errors) =
        test_parse("struct P { x: i32, y: i32 } fn make() -> P { P { x: 1, y: 2 } } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
}

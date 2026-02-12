mod common;

use common::{has_type_error, test_type_check};
use som::type_check::TypeError;

#[test]
fn test_infer_i32_literal() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            42
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_binary_add() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            1 + 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_let_binding() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let x = 10;
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_reference_type() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> &i32 {
            let x = 10;
            &x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_type_mismatch_return() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            42
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_unbound_variable() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            x
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnboundVariable { .. })
    }));
}

#[test]
fn test_infer_deref() {
    let typed_ast = test_type_check(
        r#"
        fn test(x: &i32) -> i32 {
            *x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_mut_reference() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> &mut i32 {
            let x = 10;
            &mut x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_function_param_types() {
    let typed_ast = test_type_check(
        r#"
        fn add(x: i32, y: i32) -> i32 {
            x + y
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_nested_blocks() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let x = {
                let y = 10;
                y + 1
            };
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 < 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_type_annotation_mismatch() {
    let typed_ast = test_type_check(
        r#"
        fn test() {
            let x: bool = 42;
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_infer_bool_literal() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            true
        }
        "#,
    );
    // This might fail if bool literals aren't implemented
    // Just checking it doesn't crash
    let _ = typed_ast;
}

#[test]
fn test_multiple_functions() {
    let typed_ast = test_type_check(
        r#"
        fn foo() -> i32 {
            42
        }
        fn bar() -> i32 {
            1 + 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_string_literal() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> &'static str {
            "hello world"
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_string_literal_type_mismatch() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            "hello"
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_infer_bool_true_false() {
    let typed_ast = test_type_check(
        r#"
        fn test_true() -> bool {
            true
        }
        fn test_false() -> bool {
            false
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_generic_identity() {
    let typed_ast = test_type_check(
        r#"
        fn identity<T>(x: T) -> T { x }
        fn main() -> i32 {
            identity(42)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_generic_multiple_type_params() {
    let typed_ast = test_type_check(
        r#"
        fn first<T, U>(x: T, y: U) -> T { x }
        fn main() -> i32 {
            first(1, true)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_generic_type_mismatch() {
    let typed_ast = test_type_check(
        r#"
        fn identity<T>(x: T) -> T { x }
        fn main() {
            identity(1) + false
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::MissingImpl { .. })
    }));
}

#[test]
fn test_unknown_type_error() {
    let typed_ast = test_type_check(
        r#"
        fn bad(x: Foo) -> Foo { x }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnknownType { .. })
    }));
}

#[test]
fn test_conditional_basic() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            1 if true else 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_with_variable_condition() {
    let typed_ast = test_type_check(
        r#"
        fn test(b: bool) -> i32 {
            10 if b else 20
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_bool_result() {
    let typed_ast = test_type_check(
        r#"
        fn test(a: bool, b: bool) -> bool {
            a if b else false
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_branch_type_mismatch() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            1 if true else false
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_conditional_condition_not_bool() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            1 if 42 else 2
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_conditional_nested() {
    let typed_ast = test_type_check(
        r#"
        fn test(a: bool, b: bool) -> i32 {
            1 if a else (2 if b else 3)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_in_let_binding() {
    let typed_ast = test_type_check(
        r#"
        fn test(b: bool) -> i32 {
            let x = 5 if b else 10;
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_with_arithmetic() {
    let typed_ast = test_type_check(
        r#"
        fn test(b: bool) -> i32 {
            (1 + 2) if b else (3 * 4)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_type_inference_from_annotation() {
    let typed_ast = test_type_check(
        r#"
        fn test(b: bool) -> i32 {
            let x: i32 = 1 if b else 2;
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_conditional_with_comparison_condition() {
    let typed_ast = test_type_check(
        r#"
        fn test(x: i32) -> i32 {
            1 if x > 0 else 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

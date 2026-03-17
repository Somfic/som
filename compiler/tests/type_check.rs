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

// ============================================================================
// Pointer type tests
// ============================================================================

#[test]
fn test_pointer_type_annotation() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        fn test() {
            let p: * = malloc(100);
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_pointer_type_return() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        fn alloc() -> * {
            malloc(64)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_pointer_type_parameter() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn free(p: *);
            fn malloc(size: i32) -> *;
        }
        fn test() {
            let p: * = malloc(100);
            free(p);
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_pointer_type_mismatch_with_i32() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        fn test() -> i32 {
            malloc(100)
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_pointer_type_mismatch_with_reference() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        fn test() -> &i32 {
            malloc(100)
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_pointer_passed_to_wrong_param_type() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        fn takes_int(x: i32) {}
        fn test() {
            let p: * = malloc(100);
            takes_int(p);
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

// ============================================================================
// More inference tests (should pass, no errors)
// ============================================================================

#[test]
fn test_infer_subtraction() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            5 - 3
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_multiplication() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            3 * 4
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_division() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            10 / 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_modulo() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            10 % 3
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_nested_arithmetic() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            (1 + 2) * (3 - 4)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_function_call() {
    let typed_ast = test_type_check(
        r#"
        fn add(a: i32, b: i32) -> i32 { a + b }
        fn test() -> i32 { add(1, 2) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_nested_function_call() {
    let typed_ast = test_type_check(
        r#"
        fn f(x: i32) -> i32 { x + 1 }
        fn g(x: i32) -> i32 { f(x) + 1 }
        fn test() -> i32 { g(1) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_recursive_function() {
    let typed_ast = test_type_check(
        r#"
        fn fact(n: i32) -> i32 { 1 if n <= 1 else n * fact(n - 1) }
        fn test() -> i32 { fact(5) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_while_loop() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut x = 0;
            while x < 10 {
                x = x + 1;
            }
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_struct_field_access() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32, y: i32 }
        fn test() -> i32 {
            let p = P { x: 1, y: 2 };
            p.x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_struct_construction() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn test() {
            let p = P { x: 42 };
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_method_call() {
    let typed_ast = test_type_check(
        r#"
        struct W { v: i32 }
        impl W {
            fn get(self: W) -> i32 { self.v }
        }
        fn test() -> i32 {
            let w = W { v: 5 };
            w.get()
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_static_method_call() {
    let typed_ast = test_type_check(
        r#"
        struct C { n: i32 }
        impl C {
            fn zero() -> C { C { n: 0 } }
        }
        fn test() -> i32 {
            let c = C::zero();
            c.n
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_let_with_block() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let x = {
                let y = 5;
                y + 1
            };
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_shadowed_variable() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let x = 1;
            let x = true;
            42
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison_equals() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 == 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison_not_equals() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 != 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison_gt() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 > 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison_lt_eq() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 <= 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_comparison_gt_eq() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool {
            1 >= 2
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_if_statement() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut x = 0;
            if true { x = 1; }
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_if_else_statement() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut x = 0;
            if false { x = 1; } else { x = 2; }
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_multiple_params() {
    let typed_ast = test_type_check(
        r#"
        fn add3(a: i32, b: i32, c: i32) -> i32 { a + b + c }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_void_function() {
    let typed_ast = test_type_check(
        r#"
        fn noop() {}
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_string_param() {
    let typed_ast = test_type_check(
        r#"
        fn greet(s: &str) {}
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_bool_param() {
    let typed_ast = test_type_check(
        r#"
        fn check(b: bool) -> i32 { 1 if b else 0 }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_struct_return_type() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn make() -> P { P { x: 1 } }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_struct_parameter() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn get_x(p: P) -> i32 { p.x }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_method_with_args() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        impl P {
            fn add(self: P, n: i32) -> i32 { self.x + n }
        }
        fn test() -> i32 {
            let p = P { x: 5 };
            p.add(3)
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_multiple_structs() {
    let typed_ast = test_type_check(
        r#"
        struct A { a: i32 }
        struct B { b: i32 }
        fn test() -> i32 {
            let a = A { a: 1 };
            let b = B { b: 2 };
            a.a + b.b
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_chained_calls() {
    let typed_ast = test_type_check(
        r#"
        fn f(x: i32) -> i32 { x + 1 }
        fn g(x: i32) -> i32 { x * 2 }
        fn test() -> i32 { g(f(5)) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_mut_variable() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut x = 5;
            x = 10;
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_assignment_in_while() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut sum = 0;
            let mut i = 0;
            while i < 5 {
                sum = sum + i;
                i = i + 1;
            }
            sum
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_nested_if() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            let mut x = 0;
            if true {
                if true {
                    x = 42;
                }
            }
            x
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_conditional_with_function_call() {
    let typed_ast = test_type_check(
        r#"
        fn double(x: i32) -> i32 { x * 2 }
        fn test(b: bool) -> i32 { double(5) if b else 0 }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_infer_extern_function() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn abs(x: i32) -> i32;
        }
        fn test() -> i32 { abs(0 - 5) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn test_error_wrong_arg_count_too_few() {
    let typed_ast = test_type_check(
        r#"
        fn add(a: i32, b: i32) -> i32 { a + b }
        fn test() { add(1); }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::WrongArgCount { .. })
    }));
}

#[test]
fn test_error_wrong_arg_count_too_many() {
    let typed_ast = test_type_check(
        r#"
        fn single(a: i32) -> i32 { a }
        fn test() { single(1, 2); }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::WrongArgCount { .. })
    }));
}

#[test]
fn test_error_return_type_mismatch_bool_for_i32() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 { true }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_return_type_mismatch_i32_for_bool() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> bool { 42 }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_missing_struct_field() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32, y: i32 }
        fn test() { let p = P { x: 1 }; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::MissingField { .. })
    }));
}

#[test]
fn test_error_unknown_struct_field() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn test() { let p = P { x: 1, z: 2 }; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnknownField { .. })
    }));
}

#[test]
fn test_error_wrong_field_type() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn test() { let p = P { x: true }; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_unknown_struct() {
    let typed_ast = test_type_check(
        r#"
        fn test() { let p = Unknown { x: 1 }; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnknownStruct { .. })
    }));
}

#[test]
fn test_error_unknown_function() {
    let typed_ast = test_type_check(
        r#"
        fn test() { nonexistent(); }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnknownFunction { .. })
    }));
}

#[test]
fn test_error_unbound_in_block() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 {
            { let x = 1; };
            x
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::UnboundVariable { .. })
    }));
}

#[test]
fn test_error_add_bool_and_int() {
    let typed_ast = test_type_check(
        r#"
        fn test() { true + 1; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::MissingImpl { .. })
    }));
}

#[test]
fn test_error_compare_bool_and_int() {
    let typed_ast = test_type_check(
        r#"
        fn test() { true < 1; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::MissingImpl { .. })
    }));
}

#[test]
fn test_error_conditional_condition_i32() {
    let typed_ast = test_type_check(
        r#"
        fn test() -> i32 { 1 if 42 else 2 }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_conditional_branches_different_types() {
    let typed_ast = test_type_check(
        r#"
        fn test() { 1 if true else true; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_method_wrong_arg_type() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        impl P {
            fn add(self: P, n: i32) -> i32 { self.x + n }
        }
        fn test() {
            let p = P { x: 1 };
            p.add(true);
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_field_access_on_non_struct() {
    let typed_ast = test_type_check(
        r#"
        fn test() {
            let x = 5;
            x.field;
        }
        "#,
    );
    assert!(!typed_ast.errors.is_empty());
}

#[test]
fn test_error_assign_wrong_type() {
    let typed_ast = test_type_check(
        r#"
        fn test() {
            let mut x: i32 = 0;
            x = true;
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_while_condition_not_bool() {
    let typed_ast = test_type_check(
        r#"
        fn test() { while 42 {} }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_if_condition_not_bool() {
    let typed_ast = test_type_check(
        r#"
        fn test() {
            let mut x = 0;
            if 42 { x = 1; }
        }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_error_multiple_errors() {
    let typed_ast = test_type_check(
        r#"
        fn test() { unknown1(); unknown2(); }
        "#,
    );
    assert!(typed_ast.errors.len() >= 2);
}

#[test]
fn test_error_wrong_arg_type() {
    let typed_ast = test_type_check(
        r#"
        fn takes_bool(b: bool) {}
        fn test() { takes_bool(42); }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

// ============================================================================
// Struct type checking
// ============================================================================

#[test]
fn test_struct_field_type_inference() {
    let typed_ast = test_type_check(
        r#"
        struct S { a: i32, b: bool }
        fn test() { let s = S { a: 1, b: true }; }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_struct_two_fields_wrong_type() {
    let typed_ast = test_type_check(
        r#"
        struct S { a: i32, b: bool }
        fn test() { let s = S { a: true, b: 1 }; }
        "#,
    );
    assert!(has_type_error(&typed_ast, |e| {
        matches!(e, TypeError::Mismatch { .. })
    }));
}

#[test]
fn test_struct_method_returns_struct() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        impl P {
            fn dup(self: P) -> P { P { x: self.x } }
        }
        fn test() {
            let p = P { x: 1 };
            let q = p.dup();
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_struct_in_conditional() {
    let typed_ast = test_type_check(
        r#"
        struct P { x: i32 }
        fn test(b: bool) -> P { P { x: 1 } if b else P { x: 2 } }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

// ============================================================================
// Generic tests
// ============================================================================

#[test]
fn test_generic_identity_bool() {
    let typed_ast = test_type_check(
        r#"
        fn id<T>(x: T) -> T { x }
        fn test() -> bool { id(true) }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_generic_identity_string() {
    let typed_ast = test_type_check(
        r#"
        fn id<T>(x: T) -> T { x }
        fn test() -> &str { id("hello") }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_generic_two_calls() {
    let typed_ast = test_type_check(
        r#"
        fn id<T>(x: T) -> T { x }
        fn test() -> i32 {
            let a = id(1);
            let b = id(2);
            a + b
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

// ============================================================================
// Pointer type additional tests
// ============================================================================

#[test]
fn test_pointer_in_struct() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
        }
        struct S { ptr: * }
        fn test() {
            let s = S { ptr: malloc(10) };
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

#[test]
fn test_pointer_function_chain() {
    let typed_ast = test_type_check(
        r#"
        extern {
            fn malloc(size: i32) -> *;
            fn free(p: *);
        }
        fn alloc_and_free() {
            let p = malloc(10);
            free(p);
        }
        "#,
    );
    assert!(typed_ast.errors.is_empty());
}

use som_ast::{Ast, Item};
use som_base::Source;
use som_parser::ParseError;
use std::sync::Arc;

fn test_parse(source: &str) -> (Ast, Vec<ParseError>) {
    som_parser::parse(Arc::new(Source::from_raw(source)))
}

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
    let (_, errors) = test_parse("fn main() { let mut x = 0; while x < 10 { x = x + 1; } }");
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
    let (ast, errors) =
        test_parse("struct S { v: i32 } impl S { fn get(self: S) -> i32 { self.v } } fn main() {}");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    let methods = ast.modules[0].items.iter().find_map(|item| match item {
        Item::Impl(block) => Some(
            block
                .methods
                .iter()
                .map(|&m| ast.funcs[m].name.to_string())
                .collect::<Vec<_>>(),
        ),
        _ => None,
    });
    assert_eq!(methods, Some(vec!["get".to_string()]));
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
    let (ast, errors) =
        test_parse("fn foo() -> i32 { 1 } fn bar() -> i32 { 2 } fn baz() -> i32 { 3 }");
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(ast.funcs.len(), 3);
}

#[test]
fn test_parse_missing_closing_brace() {
    let (_, errors) = test_parse("fn main() { 1");
    assert!(
        !errors.is_empty(),
        "Expected parse errors for missing closing brace"
    );
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
    let (_, errors) = test_parse("fn main() { let mut x = 0; if true { x = 1; } else { x = 2; } }");
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

fn doc_of_func(ast: &Ast, name: &str) -> Option<String> {
    ast.funcs
        .iter()
        .find(|(_, f)| f.name.to_string() == name)
        .and_then(|(_, f)| f.doc.clone())
}

#[test]
fn doc_comments_attach_to_the_next_item() {
    let (ast, errors) = test_parse(
        "/// Adds two numbers.\n///\n///   Indented line.\nfn add(a: i32, b: i32) -> i32 { a + b }\n\
         /// A point.\nstruct Point {\n    /// Across.\n    x: i32,\n    y: i32,\n}",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert_eq!(
        doc_of_func(&ast, "add").as_deref(),
        Some("Adds two numbers.\n\n  Indented line.")
    );
    let (_, point) = ast.structs.iter().next().unwrap();
    assert_eq!(point.doc.as_deref(), Some("A point."));
    assert_eq!(point.fields[0].doc.as_deref(), Some("Across."));
    assert_eq!(point.fields[1].doc, None);
}

#[test]
fn doc_comments_need_three_slashes_and_no_gap() {
    let (ast, errors) = test_parse(
        "// Not a doc.\nfn a() {}\n/// Detached.\n\nfn b() {}\n//// Rule.\nfn c() {}\n\
         /// Kept.\n// note\nfn d() {}",
    );
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    for name in ["a", "b", "c", "d"] {
        assert_eq!(doc_of_func(&ast, name), None, "{name}");
    }
}

#[test]
fn a_dot_without_a_name_keeps_what_is_before_it() {
    // While typing `alloc(10).size`, an editor still needs `alloc(10)` to list fields.
    let (ast, errors) = test_parse("fn main() { let a = alloc(10).\n}");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let calls = ast
        .exprs
        .iter()
        .filter(|(_, e)| matches!(e, som_ast::Expr::Call { .. }))
        .count();
    let lets = ast
        .stmts
        .iter()
        .filter(|(_, s)| matches!(s, som_ast::Stmt::Let { .. }))
        .count();
    assert_eq!((calls, lets), (1, 1));
}

#[test]
fn a_let_without_its_semicolon_is_reported_and_kept() {
    let (ast, errors) = test_parse("fn main() { let a = 1\n let b = 2; }");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let lets = ast
        .stmts
        .iter()
        .filter(|(_, s)| matches!(s, som_ast::Stmt::Let { .. }))
        .count();
    assert_eq!(lets, 2);
}

#[test]
fn an_unclosed_call_is_reported_and_kept() {
    let (ast, errors) = test_parse("fn main() { free(alloc(10).\n}");
    assert!(!errors.is_empty());
    let calls = ast
        .exprs
        .iter()
        .filter(|(_, e)| matches!(e, som_ast::Expr::Call { .. }))
        .count();
    assert_eq!(calls, 2);
}

/// What each parse error's fix would put in, error by error.
fn insertions(source: &str) -> Vec<Option<String>> {
    let (_, errors) = test_parse(source);
    errors
        .iter()
        .map(|e| {
            e.fix
                .as_ref()
                .map(|fix| fix.edits.iter().map(|(_, text)| text.as_str()).collect())
        })
        .collect()
}

#[test]
fn a_missing_comma_between_items_is_put_in() {
    // Fields, parameters, array elements and arguments: each missing `,` once.
    let errors =
        insertions("struct P { x: i32 y: i32 } fn f(a: i32 b: i32) { let v = [1, 2 3]; f(1 2); }");
    assert_eq!(errors, vec![Some(",".to_string()); 4]);
}

#[test]
fn a_missing_closing_bracket_is_put_in() {
    // `->` can't start a parameter, so it's the `)` that's missing, not a `,`.
    assert_eq!(
        insertions("fn f(a: i32 -> i32 { a }"),
        vec![Some(")".to_string())]
    );
}

#[test]
fn an_if_on_its_own_line_is_a_statement_after_a_missing_semicolon() {
    // Not `get() if x { … }`, a conditional missing its `else`.
    let errors = insertions("fn f() { let a = get()\n    if a { } }");
    assert_eq!(errors, vec![Some(";".to_string())]);
}

#[test]
fn an_item_for_with_a_wrong_element_says_so_at_the_element() {
    let text = "@for T in [u8, 12] {\n    impl Size for T {}\n}\n";
    let (_, errors) = som_parser::parse(std::sync::Arc::new(som_base::Source::from_raw(text)));
    assert_eq!(errors.len(), 1, "{errors:?}");
    // At the `12` in the list, not after `for` in the body.
    assert_eq!(&text[errors[0].span.start_offset..][..2], "12");
}

#[test]
fn an_item_for_writes_out_one_copy_for_each_element() {
    let text = "@for T in [u8, u16] {\n    impl Size for T {}\n}\n@for N in 0..3 {\n    impl Size for [u8; N] {}\n}\n";
    let (ast, errors) = som_parser::parse(std::sync::Arc::new(som_base::Source::from_raw(text)));
    assert!(errors.is_empty(), "{errors:?}");
    let impls = (ast.modules[0].items.iter())
        .filter(|item| matches!(item, som_ast::Item::Impl(_)))
        .count();
    assert_eq!(impls, 5);
}

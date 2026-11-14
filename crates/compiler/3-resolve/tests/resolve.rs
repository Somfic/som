use som_ast::{Ast, Expr, Item, Stmt};
use som_base::Source;
use som_resolve::{DefKind, Names, ResolveError, resolve};
use std::sync::Arc;

fn run(source: &str) -> (Ast, Names, Vec<ResolveError>) {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = resolve(&ast);
    (ast, names, errors)
}

fn ok(source: &str) -> (Ast, Names) {
    let (ast, names, errors) = run(source);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    (ast, names)
}

fn help(error: &ResolveError) -> Option<String> {
    match error {
        ResolveError::UnboundVariable { help, .. }
        | ResolveError::UnknownFunction { help, .. }
        | ResolveError::UnknownStruct { help, .. }
        | ResolveError::UnknownType { help, .. }
        | ResolveError::UnknownTrait { help, .. } => help.as_ref().map(|h| h.plain()),
        ResolveError::DuplicateDefinition { .. }
        | ResolveError::NotALoop { .. }
        | ResolveError::MethodsOnType { .. }
        | ResolveError::PropNamedLikeField { .. }
        | ResolveError::PropParams { .. }
        | ResolveError::PropGenerics { .. }
        | ResolveError::PropMutSelf { .. }
        | ResolveError::PropInTraitImpl { .. } => None,
    }
}

/// The definition each `Var` expression resolved to, in source order.
fn var_defs(ast: &Ast, names: &Names) -> Vec<DefKind> {
    ast.exprs
        .iter()
        .filter(|(_, e)| matches!(e, Expr::Var(_)))
        .map(|(id, _)| names.def(names.expr(id).expect("resolved")).kind.clone())
        .collect()
}

#[test]
fn items_can_be_used_before_they_are_defined() {
    ok("fn main() -> i32 { helper() } fn helper() -> i32 { 1 }");
}

#[test]
fn inner_let_shadows_outer() {
    let (ast, names) = ok("fn f() -> i32 { let x = 1; { let x = 2; x } }");
    let lets: Vec<_> = ast
        .stmts
        .iter()
        .filter(|(_, s)| matches!(s, Stmt::Let { .. }))
        .map(|(id, _)| id)
        .collect();
    match var_defs(&ast, &names)[..] {
        [DefKind::Local(stmt)] => assert_eq!(stmt, lets[1], "should refer to the inner `x`"),
        ref other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn let_cannot_see_itself() {
    let (_, _, errors) = run("fn f() { let x = x; }");
    assert!(
        matches!(errors[..], [ResolveError::UnboundVariable { .. }]),
        "{errors:?}"
    );
}

#[test]
fn block_locals_do_not_escape() {
    let (_, _, errors) = run("fn f() -> i32 { { let x = 1; }; x }");
    assert!(
        matches!(errors[..], [ResolveError::UnboundVariable { .. }]),
        "{errors:?}"
    );
}

#[test]
fn params_resolve_to_params() {
    let (ast, names) = ok("fn add(a: i32, b: i32) -> i32 { a + b }");
    assert!(matches!(
        var_defs(&ast, &names)[..],
        [
            DefKind::Param { index: 0, .. },
            DefKind::Param { index: 1, .. }
        ]
    ));
}

#[test]
fn methods_resolve_through_their_struct() {
    let (ast, names) =
        ok("struct S { v: i32 } impl S { fn new() -> S { S { v: 1 } } } fn main() { S::new(); }");
    let call = ast
        .exprs
        .iter()
        .find(|(_, e)| matches!(e, Expr::Call { .. }))
        .unwrap()
        .0;
    let method = match &ast.modules[0].items[1] {
        Item::Impl(block) => block.methods[0],
        _ => unreachable!(),
    };
    assert_eq!(names.expr(call), Some(names.func(method)));
}

#[test]
fn methods_are_not_visible_unqualified() {
    let (_, _, errors) =
        run("struct S { v: i32 } impl S { fn new() -> i32 { 1 } } fn main() { new(); }");
    assert!(
        matches!(errors[..], [ResolveError::UnknownFunction { .. }]),
        "{errors:?}"
    );
}

#[test]
fn unknown_method_is_reported() {
    let (_, _, errors) = run("struct S { v: i32 } fn main() { S::nope(); }");
    assert!(
        matches!(errors[..], [ResolveError::UnknownFunction { .. }]),
        "{errors:?}"
    );
}

#[test]
fn type_params_resolve() {
    let (ast, names) = ok("fn id<T>(x: T) -> T { x }");
    for (id, _) in ast.types.iter() {
        assert!(matches!(
            names.def(names.ty(id).unwrap()).kind,
            DefKind::TypeParam { index: 0, .. }
        ));
    }
}

#[test]
fn structs_are_not_values() {
    let (_, _, errors) = run("struct S { v: i32 } fn main() { let x = S; }");
    assert!(
        matches!(errors[..], [ResolveError::UnboundVariable { .. }]),
        "{errors:?}"
    );
}

#[test]
fn functions_sharing_a_name_are_overloads() {
    // Whether their types tell them apart is the type checker's to say.
    let (ast, names, errors) = run("fn f() {} fn f(x: i32) {}");
    assert!(errors.is_empty(), "{errors:?}");
    let first = names.func(ast.funcs.iter().next().unwrap().0);
    assert_eq!(names.overloads(first).len(), 2);
}

#[test]
fn duplicate_extern_declarations_are_fine() {
    let (ast, names) =
        ok("extern { fn puts(s: &str); } extern { fn puts(s: &str); } fn main() { puts(\"x\"); }");
    let externs: Vec<_> = ast
        .funcs
        .iter()
        .filter(|(_, f)| f.body.is_none())
        .map(|(id, _)| names.func(id))
        .collect();
    assert_eq!(
        externs[0], externs[1],
        "both declarations share one definition"
    );
}

#[test]
fn function_sharing_an_externs_name_overloads_it() {
    let (_, _, errors) = run("extern { fn puts(s: &str); } fn puts(n: i32) {}");
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn suggests_close_names() {
    let (_, _, errors) = run("fn helper() {} fn main() { helpr(); }");
    assert_eq!(help(&errors[0]).as_deref(), Some("did you mean `helper`?"));

    let (_, _, errors) = run("fn main() { let count = 1; let y = cuont; }");
    assert_eq!(help(&errors[0]).as_deref(), Some("did you mean `count`?"));
}

/// Each fix of the first error, as its title and the source after applying it.
fn fixes(source: &str) -> Vec<(String, String)> {
    let (_, _, errors) = run(source);
    let diagnostic = errors[0].to_diagnostic();
    diagnostic
        .fixes
        .iter()
        .map(|fix| {
            let mut text = source.to_string();
            let mut edits = fix.edits.clone();
            edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.start_offset));
            for (span, new) in edits {
                text.replace_range(span.start_offset..span.start_offset + span.length, &new);
            }
            (fix.title.plain(), text)
        })
        .collect()
}

#[test]
fn a_misspelled_name_can_be_fixed() {
    assert_eq!(
        fixes("fn helper() {} fn main() { helpr() }"),
        [(
            "change to `helper`".to_string(),
            "fn helper() {} fn main() { helper() }".to_string()
        )]
    );
}

use som_base::Source;
use som_mir::*;
use std::sync::Arc;

fn lower(source: &str) -> Mir {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    let mir = som_lower::lower(&ast, &names, &typed);
    let problems = mir.validate();
    assert!(
        problems.is_empty(),
        "invalid MIR:\n{}\n{mir}",
        problems.join("\n")
    );
    mir
}

fn func<'m>(mir: &'m Mir, name: &str) -> (FuncId, &'m Function) {
    mir.funcs
        .iter_with_ids()
        .find(|(_, f)| f.name == name)
        .unwrap_or_else(|| panic!("no {name} in\n{mir}"))
}

#[test]
fn simple_function() {
    let mir = lower("fn add(a: i32, b: i32) -> i32 { a + b }");
    assert_eq!(
        mir.display_func(func(&mir, "add").0),
        "\
fn add(_1: i32, _2: i32) -> i32 {
    let _0: i32;
    let _1: i32; // a
    let _2: i32; // b
    let _3: i32;

    bb0: {
        _3 = Add(copy _1, copy _2);
        _0 = copy _3;
        StorageDead(_2);
        StorageDead(_1);
        return;
    }
}
"
    );
}

#[test]
fn a_returned_reference_outlives_its_local() {
    // The borrow checker relies on this order: the reference is taken, then `x` dies,
    // then the reference is returned.
    let mir = lower("fn f() -> &i32 { let x = 10; &x }");
    let text = mir.display_func(func(&mir, "f").0);
    let order: Vec<usize> = ["= &_1", "StorageDead(_1)", "_0 = "]
        .iter()
        .map(|s| text.find(s).unwrap())
        .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{text}");
}

#[test]
fn structs_move_and_numbers_copy() {
    let mir = lower(
        "struct P { x: i32 } fn take(p: P) -> i32 { p.x } fn main() -> i32 { let p = P { x: 1 }; let n = 2; take(p) + n }",
    );
    let text = mir.display_func(func(&mir, "main").0);
    assert!(text.contains("call take(move _1)"), "{text}");
    assert!(text.contains("copy _2"), "{text}");
}

#[test]
fn generics_are_instantiated_per_type() {
    let mir = lower(
        "fn id<T>(x: T) -> T { x } fn unused<T>(x: T) -> T { x } fn main() -> i32 { id(true); id(1) }",
    );
    let names: Vec<&str> = mir.funcs.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"id<bool>") && names.contains(&"id<i32>"),
        "{names:?}"
    );
    assert!(
        !names.iter().any(|n| n.starts_with("unused")),
        "uncalled generics produce nothing: {names:?}"
    );
}

#[test]
fn fields_through_references_are_dereferenced() {
    let mir = lower("struct P { x: i32 } fn get(p: &P) -> i32 { p.x }");
    assert!(
        mir.display_func(func(&mir, "get").0)
            .contains("copy (*_1).0")
    );
}

#[test]
fn constructor_fields_are_put_in_declaration_order() {
    let mir = lower("struct P { x: i32, y: bool } fn f() -> P { P { y: true, x: 1 } }");
    assert!(
        mir.display_func(func(&mir, "f").0)
            .contains("P { x: const 1_i32, y: const true }")
    );
}

#[test]
fn entry_point_and_externs() {
    let mir = lower("extern { fn puts(s: cstr); } fn main() { puts(\"hi\"); }");
    assert!(func(&mir, "main").1.is_entry);
    let puts = func(&mir, "puts").1;
    assert!(puts.body.is_none());
    assert_eq!(puts.link_name.as_deref(), Some("puts"));
}

#[test]
fn validator_catches_ill_typed_mir() {
    let mut mir = lower("fn f() -> i32 { 1 }");
    let bool = mir.types.bool;
    let body = mir.funcs.iter_mut().next().unwrap().body.as_mut().unwrap();
    let StatementKind::Assign(_, Rvalue::Use(Operand::Const(c))) =
        &mut body.blocks[Id::new(0)].stmts[0].kind
    else {
        panic!("expected `_0 = const 1`");
    };
    c.ty = bool;
    assert!(!mir.validate().is_empty());
}

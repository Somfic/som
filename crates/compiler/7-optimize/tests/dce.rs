use som_base::Source;
use som_mir::Mir;
use std::sync::Arc;

/// The MIR of `source` after every optimization.
fn optimized(source: &str) -> String {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    let mut mir: Mir = som_lower::lower(&ast, &names, &typed);
    som_optimize::optimize(&mut mir);
    let problems = mir.validate();
    assert!(
        problems.is_empty(),
        "invalid MIR:\n{}\n{mir}",
        problems.join("\n")
    );
    mir.to_string()
}

/// The MIR of the function `name` in `mir`.
fn func<'a>(mir: &'a str, name: &str) -> &'a str {
    let start = mir.find(&format!("fn {name}(")).expect("function exists");
    let end = mir[start..].find("\n}\n").map_or(mir.len(), |e| start + e);
    &mir[start..end]
}

#[test]
fn unused_variables_are_removed() {
    let mir = optimized(
        "fn work(a: i32, b: i32) -> i32 { let sum = a + b; let doubled = sum * 2; let flag = a < b; a - b }
         fn main() -> i32 { work(3, 4) }",
    );
    let work = func(&mir, "work");
    for gone in ["sum", "doubled", "flag", "Add", "Multiply", "LessThan"] {
        assert!(!work.contains(gone), "`{gone}` should be gone:\n{work}");
    }
    assert!(work.contains("Subtract"), "{work}");
}

#[test]
fn constants_folding_left_unread_are_removed() {
    let mir = optimized("fn main() -> i32 { let a = 2; let b = a * 10; b + 1 }");
    assert!(mir.contains("_0 = const 21_i32"), "{mir}");
    assert!(!mir.contains("const 2_i32"), "{mir}");
    assert!(!mir.contains("const 20_i32"), "{mir}");
}

#[test]
fn locals_that_only_feed_themselves_are_removed() {
    let mir = optimized(
        "fn main(n: i32) -> i32 { let mut i = 0; while i < n { i = i + 1; } let mut c = 0; let mut j = 0; while j < n { c = c + 1; j = j + 1; } 0 }",
    );
    assert!(mir.contains("// i"), "the loop condition reads `i`:\n{mir}");
    assert!(!mir.contains("// c"), "nothing reads `c`:\n{mir}");
}

#[test]
fn unused_structs_and_borrows_are_removed_and_shaken() {
    let mir = optimized(
        "struct Point { x: i32, y: i32 }
         fn main(a: i32) -> i32 { let p = Point { x: a, y: a }; let r = &p; a }",
    );
    assert!(!mir.contains("Point"), "{mir}");
}

#[test]
fn calls_writes_through_references_and_traps_stay() {
    let mir = optimized(
        "extern { fn puts(s: &str); }
         fn set(x: &mut i32) { *x = 5; }
         fn main(a: i32) -> i32 { let unused = puts(\"hi\"); let q = 10 / a; let mut v = 1; set(&mut v); 0 }",
    );
    assert!(mir.contains("call puts"), "{mir}");
    assert!(mir.contains("Divide("), "dividing by `a` can trap:\n{mir}");
    assert!(func(&mir, "set").contains("(*_1) = const 5_i32"), "{mir}");
    assert!(mir.contains("call set"), "{mir}");
}

#[test]
fn unused_parameters_are_kept() {
    let mir = optimized("fn f(a: i32, b: i32) -> i32 { a } fn main() -> i32 { f(1, 2) }");
    assert!(
        func(&mir, "f").starts_with("fn f(_1: i32, _2: i32)"),
        "{mir}"
    );
}

#[test]
fn overwritten_stores_are_removed() {
    let mir = optimized(
        "fn f(a: i32, b: i32) -> i32 { let mut x = a * 3; x = b * 5; x }
         fn main() -> i32 { f(1, 2) }",
    );
    let f = func(&mir, "f");
    assert!(
        !f.contains("Multiply(copy _1"),
        "`a * 3` is overwritten:\n{f}"
    );
    assert!(f.contains("Multiply(copy _2"), "{f}");
}

#[test]
fn stores_read_on_some_path_stay() {
    let mir = optimized(
        "fn f(a: i32, c: bool) -> i32 { let mut x = a * 3; if c { x = a * 5; } x }
         fn main() -> i32 { f(1, true) }",
    );
    let f = func(&mir, "f");
    assert!(
        f.contains("const 3_i32"),
        "`x` is read when `c` is false:\n{f}"
    );
    assert!(f.contains("const 5_i32"), "{f}");
}

#[test]
fn stores_before_a_loop_reads_them_stay() {
    let mir = optimized(
        "fn f(n: i32) -> i32 { let mut s = n * 2; let mut i = 0; while i < n { s = s + i; i = i + 1; } s }
         fn main() -> i32 { f(3) }",
    );
    assert!(func(&mir, "f").contains("Multiply"), "{mir}");
}

#[test]
fn stores_to_borrowed_locals_stay() {
    let mir = optimized(
        "fn read(r: &i32) -> i32 { *r }
         fn f(a: i32) -> i32 { let mut x = a * 7; let r = &x; read(r) }
         fn main() -> i32 { f(1) }",
    );
    assert!(func(&mir, "f").contains("Multiply"), "{mir}");
}

#[test]
fn field_stores_that_are_overwritten_whole_are_removed() {
    let mir = optimized(
        "struct P { x: i32, y: i32 }
         fn f(a: i32) -> i32 { let mut p = P { x: a, y: a }; p.x = a * 9; p = P { x: 1, y: a }; p.y }
         fn main() -> i32 { f(1) }",
    );
    assert!(!func(&mir, "f").contains("const 9_i32"), "{mir}");
}

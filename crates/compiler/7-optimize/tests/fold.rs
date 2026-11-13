use som_base::Source;
use som_mir::Mir;
use std::sync::Arc;

/// The MIR of `main` in `source`, after folding.
fn folded(source: &str) -> String {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    let mut mir: Mir = som_lower::lower(&ast, &names, &typed);
    som_optimize::fold(&mut mir);
    let problems = mir.validate();
    assert!(
        problems.is_empty(),
        "invalid MIR:\n{}\n{mir}",
        problems.join("\n")
    );
    mir.to_string()
}

#[track_caller]
fn assert_returns(source: &str, value: &str) {
    let mir = folded(source);
    assert!(
        mir.contains(&format!("_0 = {value}")),
        "expected `_0 = {value}` in\n{mir}"
    );
}

#[test]
fn arithmetic_on_constants_is_folded() {
    assert_returns("fn main() -> i32 { 1 + 2 * 3 }", "const 7_i32");
}

#[test]
fn constant_locals_are_propagated_and_removed() {
    let mir = folded("fn main() -> i32 { let a = 2; let b = a * 10; b + 1 }");
    assert!(mir.contains("_0 = const 21_i32"), "{mir}");
    assert!(
        !mir.contains("= const 2_i32"),
        "the dead `a = 2` should be gone:\n{mir}"
    );
}

#[test]
fn integers_wrap_to_their_type() {
    assert_returns(
        "fn main() -> u8 { let a: u8 = 200; a + 100 }",
        "const 44_u8",
    );
    assert_returns(
        "fn main() -> i32 { let a: i32 = 2147483647; a + 1 }",
        "const -2147483648_i32",
    );
}

#[test]
fn division_that_traps_is_left_alone() {
    let mir = folded("fn main() -> i32 { let z = 0; 1 / z }");
    assert!(mir.contains("Divide(const 1_i32, const 0_i32)"), "{mir}");
    let mir = folded("fn main() -> i32 { let m: i32 = 0 - 2147483647 - 1; m / (0 - 1) }");
    assert!(
        mir.contains("Divide(const -2147483648_i32, const -1_i32)"),
        "{mir}"
    );
}

#[test]
fn division_truncates_towards_zero() {
    assert_returns("fn main() -> i32 { let a = 0 - 7; a / 2 }", "const -3_i32");
    assert_returns("fn main() -> i32 { let a = 0 - 7; a % 2 }", "const -1_i32");
}

#[test]
fn mutated_and_borrowed_locals_are_not_propagated() {
    let mir = folded("fn main() -> i32 { let mut a = 1; a = 2; a }");
    assert!(!mir.contains("_0 = const"), "{mir}");
    let mir = folded(
        "fn bump(x: &mut i32) { *x = *x + 1; }
         fn main() -> i32 { let mut a = 1; bump(&mut a); a }",
    );
    let main = &mir[mir.find("fn main").unwrap()..];
    assert!(!main.contains("_0 = const"), "{main}");
}

#[test]
fn constant_branches_become_jumps_and_dead_blocks_go() {
    let mir = folded(
        "extern { fn puts(s: &str); }
         fn main() -> i32 { if 1 < 2 { puts(\"always\"); } else { puts(\"never\"); } 0 }",
    );
    assert!(mir.contains("always"), "{mir}");
    assert!(
        !mir.contains("never"),
        "the else block should be gone:\n{mir}"
    );
    assert!(!mir.contains("branch"), "{mir}");
    assert_returns("fn main() -> i32 { 10 if 1 < 2 else 20 }", "const 10_i32");
}

#[test]
fn floats_fold_but_not_to_infinity() {
    assert_returns("fn main() -> f64 { 1.5 * 2.0 }", "const 3.0_f64");
    let mir = folded("fn main() -> f64 { 1.0 / 0.0 }");
    assert!(mir.contains("Divide("), "{mir}");
}

#[test]
fn narrow_min_divided_by_minus_one_is_left_alone() {
    // C widens `i8` first and gets 128 → -128, but other backends trap.
    let mir = folded("fn main() -> i8 { let m: i8 = 0 - 127 - 1; m / (0 - 1) }");
    assert!(mir.contains("Divide(const -128_i8, const -1_i8)"), "{mir}");
}

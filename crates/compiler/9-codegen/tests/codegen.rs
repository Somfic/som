use som_base::Source;
use som_codegen::{CodegenError, emit_c};
use std::sync::Arc;

fn c(source: &str) -> Result<String, CodegenError> {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    emit_c(&som_lower::lower(&ast, &names, &typed))
}

#[test]
fn a_program_needs_a_main() {
    assert_eq!(
        c("fn helper() -> i32 { 1 }"),
        Err(CodegenError::NoEntryPoint)
    );
}

#[test]
fn unit_main_exits_with_zero() {
    let c = c("fn main() {}").unwrap();
    assert!(c.contains("    som_main();\n    return 0;"), "{c}");
}

#[test]
fn strings_are_escaped() {
    let c = c(r#"extern { fn puts(s: cstr); } fn main() { puts("tab	quote\" é"); }"#).unwrap();
    assert!(c.contains(r#""tab\011quote\" \303\251""#), "{c}");
}

#[test]
fn externs_are_declared_once_and_return_void_for_unit() {
    let c =
        c("extern { fn puts(s: cstr); } extern { fn puts(s: cstr); } fn main() { puts(\"x\"); }")
            .unwrap();
    assert_eq!(c.matches("void puts(uint8_t*);").count(), 1, "{c}");
    assert!(
        c.contains("\n    puts((uint8_t*)\"x\");\n"),
        "unused results are plain calls:\n{c}"
    );
}

#[test]
fn functions_and_structs_keep_their_som_names() {
    let c = c(
        "struct Point { x: i32 } impl Point { fn get(self: &Point) -> i32 { self.x } }
               fn id<T>(v: T) -> T { v } fn int() -> i32 { 1 }
               fn main() -> i32 { let p = Point { x: id(1) }; (&p).get() + int() }",
    )
    .unwrap();
    for expected in [
        "struct Point { int32_t x; };",
        "static int32_t Point_get(struct Point* self)",
        "static int32_t id_i32(int32_t v)",
        // `int` is a C keyword.
        "static int32_t int_2(void)",
        "static int32_t som_main(void)",
    ] {
        assert!(c.contains(expected), "missing `{expected}` in\n{c}");
    }
}

#[test]
fn temporaries_fold_into_their_use() {
    let c =
        c("struct P { x: i32, y: i32 } fn sum(p: &P) -> i32 { p.x + p.y } fn main() -> i32 { 0 }")
            .unwrap();
    assert!(
        c.contains("static int32_t sum(struct P* p) {\n    return p->x + p->y;\n}"),
        "{c}"
    );
}

#[test]
fn literals_are_plain() {
    let c = c("fn main() -> i64 { let big: i64 = 3000000000; big + 1 }").unwrap();
    assert!(
        c.contains("big = 3000000000;") && c.contains("return big + 1;"),
        "{c}"
    );
}

#[test]
fn only_jump_targets_get_labels() {
    let straight = c("fn main() -> i32 { 1 + 2 }").unwrap();
    assert!(!straight.contains("bb"), "{straight}");

    let looping = c("fn main() -> i32 { let mut i = 0; while i < 3 { i = i + 1; } i }").unwrap();
    assert!(
        !looping.contains("bb0:"),
        "nothing jumps to the entry block:\n{looping}"
    );
    for line in looping.lines().filter(|l| l.ends_with(':')) {
        let label = line.trim_end_matches(':');
        assert!(
            looping.contains(&format!("goto {label};")),
            "unused label {label}:\n{looping}"
        );
    }
}

#[test]
fn locals_keep_their_som_names() {
    let c = c(
        "fn add(left: i32, right: i32) -> i32 { let sum = left + right; sum }
               fn main() -> i32 { add(1, 2) }",
    )
    .unwrap();
    assert!(
        c.contains("static int32_t add(int32_t left, int32_t right) {"),
        "{c}"
    );
    // Declared where it's assigned.
    assert!(c.contains("    int32_t sum = left + right;\n"), "{c}");
}

#[test]
fn clashing_local_names_get_a_suffix() {
    let c = c("extern { fn abs(x: i32) -> i32; }
               fn main() -> i32 { let int = 1; let int = int + 1; let abs = abs(int); let _1 = abs; _1 }")
    .unwrap();
    for declaration in [
        "int32_t int_2 = ",
        "int32_t int_3 = ",
        "int32_t abs_2 = ",
        "int32_t v_1 = ",
    ] {
        assert!(c.contains(declaration), "missing `{declaration}` in\n{c}");
    }
    assert!(!c.contains("int32_t int "), "{c}");
}

#[test]
fn mut_ref_parameters_are_restrict() {
    let c = c("extern { fn memset(p: &mut u8, v: i32, n: usize); }
               fn set(a: &mut i32, b: &i32) { *a = *b; }
               fn main() { let mut x = 1; set(&mut x, &2); }")
    .unwrap();
    assert!(
        c.contains("set(int32_t* restrict a, int32_t* b)"),
        "only `&mut` is restrict:\n{c}"
    );
    assert!(
        c.contains("memset(uint8_t*, int32_t, uintptr_t)"),
        "extern signatures stay as C declares them:\n{c}"
    );
}

#[test]
fn call_results_are_named_after_the_function() {
    let c = c("fn width() -> i32 { 2 } fn height() -> i32 { 3 }
               fn main() -> i32 { width() * height() }")
    .unwrap();
    // Calls stay out of `*`, since C doesn't fix which side is evaluated first.
    assert!(c.contains("    int32_t width_2 = width();\n"), "{c}");
    assert!(c.contains("    int32_t height_2 = height();\n"), "{c}");
    assert!(c.contains("    return width_2 * height_2;\n"), "{c}");
}

#[test]
fn pure_temporaries_fold_past_calls() {
    let c = c("fn f() -> i32 { 2 }
               fn main() -> i32 { let a = 1; let b = 2; a * b + f() }")
    .unwrap();
    // `a * b` reads nothing the call can change, so it waits for `+`.
    assert!(c.contains("return a * b + f_2;"), "{c}");
}

#[test]
fn temporaries_wait_for_assignments_to_what_they_read() {
    let c = c("fn main() -> i32 { let mut a = 1; let b = a * 2; a = 5; b + a }").unwrap();
    assert!(c.contains("int32_t b = a * 2;"), "{c}");
    assert!(c.contains("return b + a;"), "{c}");
}

#[test]
fn parentheses_only_where_precedence_needs_them() {
    let c = c(
        "fn f(a: i32, b: i32, c: i32) -> bool { a * b + c < a - (b - c) && a == b }
               fn main() -> i32 { 0 }",
    )
    .unwrap();
    assert!(c.contains("if (a * b + c < a - (b - c)) goto"), "{c}");
}

#[test]
fn indexes_fold_into_the_brackets() {
    let c = c("fn second(xs: &[i32; 3]) -> i32 { xs[1] } fn main() -> i32 { 0 }").unwrap();
    assert!(c.contains("xs->items[1]"), "{c}");
}

#[test]
fn externs_come_before_the_programs_functions() {
    let c = c("extern { fn abs(x: i32) -> i32; } fn main() -> i32 { abs(1) }").unwrap();
    let (externs, own) = (
        c.find("int32_t abs(int32_t);\n").unwrap(),
        c.find("som_main(void);").unwrap(),
    );
    assert!(externs < own, "{c}");
}

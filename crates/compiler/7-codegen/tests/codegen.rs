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
    assert!(c.contains("som_f0_main();\n    return 0;"), "{c}");
}

#[test]
fn strings_are_escaped() {
    let c = c(r#"extern { fn puts(s: &str); } fn main() { puts("tab	quote\" é"); }"#).unwrap();
    assert!(c.contains(r#""tab\011quote\\\" \303\251""#), "{c}");
}

#[test]
fn externs_are_declared_once_and_return_void_for_unit() {
    let c =
        c("extern { fn puts(s: &str); } extern { fn puts(s: &str); } fn main() { puts(\"x\"); }")
            .unwrap();
    assert_eq!(c.matches("void puts(uint8_t*);").count(), 1, "{c}");
    assert!(c.contains("puts((uint8_t*)\"x\"); "), "{c}");
}

#[test]
fn generated_functions_cannot_clash_with_c_names() {
    let c = c("fn abs(x: i32) -> i32 { x } fn main() -> i32 { abs(1) }").unwrap();
    assert!(c.contains("static int32_t som_f0_abs(int32_t _1)"), "{c}");
}

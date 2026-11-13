use som_base::Source;
use som_mir::Mir;
use std::sync::Arc;

fn shaken(source: &str) -> Mir {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let (names, errors) = som_resolve::resolve(&ast);
    assert!(errors.is_empty(), "resolve errors: {errors:?}");
    let (typed, errors) = som_typeck::check(&ast, &names);
    assert!(errors.is_empty(), "type errors: {errors:?}");
    let mut mir = som_lower::lower(&ast, &names, &typed);
    som_shake::shake(&mut mir);
    let problems = mir.validate();
    assert!(problems.is_empty(), "invalid MIR:\n{}\n{mir}", problems.join("\n"));
    mir
}

fn funcs(mir: &Mir) -> Vec<&str> {
    mir.funcs.iter().map(|f| f.name.as_str()).collect()
}

fn structs(mir: &Mir) -> Vec<&str> {
    mir.structs.iter().map(|s| s.name.as_str()).collect()
}

#[test]
fn only_functions_reachable_from_main_stay() {
    let mir = shaken(
        "fn used() -> i32 { helper() } fn helper() -> i32 { 1 }
         fn dead() -> i32 { only_dead_calls_me() } fn only_dead_calls_me() -> i32 { 2 }
         fn main() -> i32 { used() }",
    );
    assert_eq!(funcs(&mir), ["used", "helper", "main"]);
}

#[test]
fn only_called_externs_stay() {
    let mir = shaken(
        "extern { fn puts(s: &str); fn abs(x: i32) -> i32; fn rand() -> i32; }
         fn main() { puts(\"x\"); }",
    );
    assert_eq!(funcs(&mir), ["puts", "main"]);
}

#[test]
fn functions_used_as_values_stay() {
    let mir = shaken(
        "fn one() -> i32 { 1 } fn unused() -> i32 { 2 }
         fn main() -> i32 { let f = one; 0 }",
    );
    assert_eq!(funcs(&mir), ["one", "main"]);
}

#[test]
fn only_structs_something_uses_stay() {
    let mir = shaken(
        "struct Inner { x: i32 } struct Outer { inner: Inner } struct Pointee { y: i32 }
         struct Holder { p: &Pointee } struct Unused { z: i32 } struct OnlyExtern { w: i32 }
         extern { fn draw(o: Outer); fn load() -> OnlyExtern; fn hold(h: &Holder); }
         fn main() { draw(Outer { inner: Inner { x: 1 } }); }",
    );
    assert_eq!(structs(&mir), ["Inner", "Outer"]);
}

#[test]
fn a_program_without_main_is_left_alone() {
    let mir = shaken("fn a() -> i32 { 1 } fn b() -> i32 { 2 }");
    assert_eq!(funcs(&mir), ["a", "b"]);
}

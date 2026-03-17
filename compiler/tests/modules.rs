use som::{LoadErrors, ProgramError, ProgramLoader, TypeInferencer};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Helper to create a test project with files
struct TestProject {
    dir: TempDir,
}

impl TestProject {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    fn add_file(&self, path: &str, content: &str) {
        let full_path = self.dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full_path, content).unwrap();
    }

    fn load(&self) -> Result<som::Ast, LoadErrors> {
        let loader = ProgramLoader::new(self.dir.path().to_path_buf());
        loader.load_project()
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

// ============================================================================
// Basic module loading
// ============================================================================

#[test]
fn test_single_file_project() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        fn main() -> i32 {
            42
        }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    assert_eq!(ast.mods.len(), 1);
    assert_eq!(ast.funcs.len(), 1);
}

#[test]
fn test_multiple_files_in_root() {
    let project = TestProject::new();
    project.add_file("main.som", "fn main() -> i32 { helper() }");
    project.add_file("helper.som", "fn helper() -> i32 { 42 }");

    let ast = project.load().expect("should load successfully");
    // Each file creates a module entry (both under root module name)
    assert_eq!(ast.mods.len(), 2);
    assert_eq!(ast.funcs.len(), 2);
}

// ============================================================================
// Use statements and dependencies
// ============================================================================

#[test]
fn test_use_loads_dependency() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use math;
        fn main() -> i32 { math::add(1, 2) }
        "#,
    );
    project.add_file("math/lib.som", "fn add(a: i32, b: i32) -> i32 { a + b }");

    let ast = project.load().expect("should load successfully");
    assert_eq!(ast.mods.len(), 2); // root + math
    assert_eq!(ast.funcs.len(), 2); // main + add
}

#[test]
fn test_use_qualified_path() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use utils;
        fn main() { utils::print(); }
        "#,
    );
    project.add_file("utils/io.som", "fn print() {}");

    let ast = project.load().expect("should load successfully");
    assert_eq!(ast.mods.len(), 2);
}

#[test]
fn test_multiple_use_statements() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use math;
        use strings;
        fn main() {}
        "#,
    );
    project.add_file("math/lib.som", "fn add(a: i32, b: i32) -> i32 { a + b }");
    project.add_file("strings/lib.som", "fn concat() {}");

    let ast = project.load().expect("should load successfully");
    assert_eq!(ast.mods.len(), 3); // root + math + strings
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn test_missing_module_error() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use nonexistent;
        fn main() {}
        "#,
    );

    match project.load() {
        Ok(_) => panic!("should fail with missing module"),
        Err(errors) => {
            assert!(errors.parse.is_empty());
            assert_eq!(errors.program.len(), 1);
            match &errors.program[0] {
                ProgramError::ModuleNotFound { name, .. } => {
                    assert_eq!(name, "nonexistent");
                }
                other => panic!("expected ModuleNotFound, got {:?}", other),
            }
        }
    }
}

// TODO: Circular dependency detection needs work - paths are relative to current folder,
// making it hard to create actual cycles with the current path resolution.
// The `loading` set tracks absolute paths but `use x` resolves relative to the file's folder.
#[test]
#[ignore = "circular dependency detection needs path resolution rework"]
fn test_circular_dependency_error() {
    // This test is a placeholder until path resolution is more sophisticated
}

#[test]
fn test_empty_module_directory() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use empty;
        fn main() {}
        "#,
    );
    // Create empty directory
    fs::create_dir_all(project.path().join("empty")).unwrap();

    // Should succeed - empty directory has no .som files, so no module entry
    let ast = project.load().expect("empty module should load");
    assert_eq!(ast.mods.len(), 1); // just root (empty dir has no files)
    assert_eq!(ast.funcs.len(), 1); // just main
}

// ============================================================================
// Module naming and registry
// ============================================================================

#[test]
fn test_function_registry_qualified_names() {
    let project = TestProject::new();
    project.add_file("main.som", "use math; fn main() {}");
    project.add_file("math/lib.som", "fn add() {}");

    let ast = project.load().expect("should load");

    // Functions should be registered with qualified names
    let project_name = project.path().file_name().unwrap().to_str().unwrap();

    // Check that math::add is registered
    assert!(
        ast.func_registry.contains_key("math::add"),
        "expected 'math::add' in registry, got: {:?}",
        ast.func_registry.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_same_function_name_different_modules() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use a;
        use b;
        fn main() {}
        "#,
    );
    project.add_file("a/lib.som", "fn helper() -> i32 { 1 }");
    project.add_file("b/lib.som", "fn helper() -> i32 { 2 }");

    let ast = project.load().expect("should load");

    // Both should be registered with different qualified names
    assert!(ast.func_registry.contains_key("a::helper"));
    assert!(ast.func_registry.contains_key("b::helper"));
    assert_eq!(ast.funcs.len(), 3); // main + a::helper + b::helper
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_deeply_nested_use() {
    let project = TestProject::new();
    project.add_file("main.som", "use a; fn main() {}");
    project.add_file("a/lib.som", "use b; fn a_func() {}");
    project.add_file("a/b/lib.som", "use c; fn b_func() {}");
    project.add_file("a/b/c/lib.som", "fn c_func() {}");

    let ast = project.load().expect("should load deeply nested modules");
    assert_eq!(ast.mods.len(), 4);
}

#[test]
fn test_module_loaded_only_once() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use shared;
        use a;
        use b;
        fn main() {}
        "#,
    );
    project.add_file("shared/lib.som", "fn shared_func() {}");
    project.add_file("a/lib.som", "use shared; fn a_func() {}");
    project.add_file("b/lib.som", "use shared; fn b_func() {}");

    let ast = project.load().expect("should load");

    // shared should only be loaded once despite being used 3 times
    let shared_count = ast.mods.iter().filter(|m| &*m.name == "shared").count();
    assert_eq!(
        shared_count, 1,
        "shared module should be loaded exactly once"
    );
}

#[test]
fn test_non_som_files_ignored() {
    let project = TestProject::new();
    project.add_file("main.som", "fn main() {}");
    project.add_file("readme.txt", "This is not a som file");
    project.add_file("notes.md", "# Notes");

    let ast = project.load().expect("should ignore non-.som files");
    assert_eq!(ast.funcs.len(), 1);
}

// ============================================================================
// Bundled std module tests
// ============================================================================

#[test]
fn test_bundled_std_loads() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {}
        "#,
    );

    let ast = project.load().expect("should load bundled std");
    // Should have at least 2 modules: main project + std
    assert!(ast.mods.len() >= 2);
}

#[test]
fn test_bundled_std_println() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {
            std::println("hello");
        }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    // Check that std::println is in the registry
    assert!(ast.func_registry.contains_key("std::println"));
}

#[test]
fn test_bundled_std_malloc_free() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {
            let p: * = std::malloc(100);
            std::free(p);
        }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    assert!(ast.func_registry.contains_key("std::malloc"));
    assert!(ast.func_registry.contains_key("std::free"));
}

#[test]
fn test_bundled_std_exit() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {
            std::exit(0);
        }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    assert!(ast.func_registry.contains_key("std::exit"));
}

#[test]
fn test_bundled_std_overrides_local() {
    // Bundled std should take precedence over local std folder
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {
            std::println("test");
        }
        "#,
    );
    // Create a local std with different content
    project.add_file("std/io.som", "fn local_only() {}");

    let ast = project.load().expect("should load successfully");
    // Should have bundled std::println, not local
    assert!(ast.func_registry.contains_key("std::println"));
    // Should NOT have local_only since bundled std takes precedence
    assert!(!ast.func_registry.contains_key("std::local_only"));
}

#[test]
fn test_extern_functions_have_qualified_names() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {}
        "#,
    );

    let ast = project.load().expect("should load successfully");
    // Extern functions should be registered with qualified names
    assert!(ast.func_registry.contains_key("std::puts"));
    assert!(ast.func_registry.contains_key("std::malloc"));
    assert!(ast.func_registry.contains_key("std::free"));
    assert!(ast.func_registry.contains_key("std::exit"));
}

#[test]
fn test_module_internal_calls_work() {
    // Functions within a module should be able to call each other unqualified
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use mymod;
        fn main() {
            mymod::outer();
        }
        "#,
    );
    project.add_file(
        "mymod/lib.som",
        r#"
        fn inner() -> i32 { 42 }
        fn outer() -> i32 { inner() }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    assert!(ast.func_registry.contains_key("mymod::inner"));
    assert!(ast.func_registry.contains_key("mymod::outer"));
}

// ============================================================================
// Unqualified access to imported module functions
// ============================================================================

#[test]
fn test_use_std_println_unqualified() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use std;
        fn main() {
            println("Hello");
        }
        "#,
    );

    let ast = project.load().expect("should load successfully");
    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    assert!(
        typed_ast.errors.is_empty(),
        "expected no type errors when calling println with `use std;`, got: {:?}",
        typed_ast.errors
    );
}

// ============================================================================
// Additional module tests
// ============================================================================

#[test]
fn test_module_with_struct() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use shapes;
        fn main() {}
        "#,
    );
    project.add_file(
        "shapes/lib.som",
        r#"
        struct Point { x: i32, y: i32 }
        fn origin() -> i32 { 0 }
        "#,
    );

    let ast = project.load().expect("should load module with struct");
    assert!(ast.func_registry.contains_key("shapes::origin"));
}

#[test]
fn test_multiple_modules_shared_dependency() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use shared;
        use a;
        use b;
        fn main() {}
        "#,
    );
    project.add_file("a/lib.som", "use shared; fn a_func() -> i32 { 1 }");
    project.add_file("a/shared/lib.som", "fn common() -> i32 { 0 }");
    project.add_file("b/lib.som", "use shared; fn b_func() -> i32 { 2 }");
    project.add_file("b/shared/lib.som", "fn common() -> i32 { 0 }");
    project.add_file("shared/lib.som", "fn common() -> i32 { 0 }");

    let ast = project.load().expect("should load");
    // All three uses resolve - main, a, and b each load their own "shared" path,
    // but main's shared is at root level
    assert!(ast.func_registry.contains_key("shared::common"));
}

#[test]
fn test_module_function_calls_other_module_function() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use a;
        fn main() { a::call_b(); }
        "#,
    );
    project.add_file(
        "a/lib.som",
        r#"
        use b;
        fn call_b() -> i32 { b::value() }
        "#,
    );
    // b is inside a's directory since `use b` from a/lib.som resolves relative to a/
    project.add_file("a/b/lib.som", "fn value() -> i32 { 99 }");

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("a::call_b"));
    assert!(ast.func_registry.contains_key("b::value"));
}

#[test]
fn test_module_with_multiple_files() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use mymod;
        fn main() { mymod::greet(); }
        "#,
    );
    project.add_file("mymod/lib.som", "fn greet() {}");
    project.add_file("mymod/helper.som", "fn assist() -> i32 { 1 }");

    let ast = project.load().expect("should load module with multiple files");
    assert!(ast.func_registry.contains_key("mymod::greet"));
    assert!(ast.func_registry.contains_key("mymod::assist"));
}

#[test]
fn test_module_exports_multiple_functions() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use calc;
        fn main() {}
        "#,
    );
    project.add_file(
        "calc/lib.som",
        r#"
        fn add(a: i32, b: i32) -> i32 { a + b }
        fn sub(a: i32, b: i32) -> i32 { a - b }
        fn mul(a: i32, b: i32) -> i32 { a * b }
        "#,
    );

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("calc::add"));
    assert!(ast.func_registry.contains_key("calc::sub"));
    assert!(ast.func_registry.contains_key("calc::mul"));
}

#[test]
fn test_use_and_local_functions() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use utils;
        fn local_helper() -> i32 { 10 }
        fn main() -> i32 { local_helper() }
        "#,
    );
    project.add_file("utils/lib.som", "fn tool() -> i32 { 5 }");

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("utils::tool"));
    assert_eq!(ast.funcs.len(), 3); // main + local_helper + tool
}

#[test]
fn test_module_with_extern() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use mylib;
        fn main() { mylib::get_abs(); }
        "#,
    );
    project.add_file(
        "mylib/lib.som",
        r#"
        extern {
            fn abs(x: i32) -> i32;
        }
        fn get_abs() -> i32 { abs(0 - 5) }
        "#,
    );

    let ast = project.load().expect("should load module with extern");
    assert!(ast.func_registry.contains_key("mylib::get_abs"));
}

#[test]
fn test_deeply_nested_three_levels() {
    let project = TestProject::new();
    project.add_file("main.som", "use a; fn main() {}");
    project.add_file("a/lib.som", "use b; fn fa() {}");
    project.add_file("a/b/lib.som", "use c; fn fb() {}");
    project.add_file("a/b/c/lib.som", "use d; fn fc() {}");
    project.add_file("a/b/c/d/lib.som", "fn fd() {}");

    let ast = project.load().expect("should load deeply nested modules");
    assert_eq!(ast.mods.len(), 5);
}

#[test]
fn test_empty_main_with_module() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use helper;
        fn main() {}
        "#,
    );
    project.add_file("helper/lib.som", "fn do_nothing() {}");

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("helper::do_nothing"));
    assert_eq!(ast.funcs.len(), 2);
}

#[test]
fn test_module_function_name_collision_with_main() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use a;
        fn helper() -> i32 { 1 }
        fn main() -> i32 { helper() }
        "#,
    );
    project.add_file("a/lib.som", "fn helper() -> i32 { 2 }");

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("a::helper"));
    assert_eq!(ast.funcs.len(), 3); // main + root helper + a::helper
}

#[test]
fn test_two_modules_each_with_structs() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use geo;
        use color;
        fn main() {}
        "#,
    );
    project.add_file(
        "geo/lib.som",
        r#"
        struct Point { x: i32, y: i32 }
        fn make_point() -> i32 { 0 }
        "#,
    );
    project.add_file(
        "color/lib.som",
        r#"
        struct Color { r: i32, g: i32, b: i32 }
        fn make_color() -> i32 { 0 }
        "#,
    );

    let ast = project.load().expect("should load modules with structs");
    assert!(ast.func_registry.contains_key("geo::make_point"));
    assert!(ast.func_registry.contains_key("color::make_color"));
}

#[test]
fn test_module_with_qualified_call_type_checks() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use math;
        fn main() -> i32 { math::add(1, 2) }
        "#,
    );
    project.add_file("math/lib.som", "fn add(a: i32, b: i32) -> i32 { a + b }");

    let ast = project.load().expect("should load");
    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    assert!(
        typed_ast.errors.is_empty(),
        "expected no type errors for qualified call, got: {:?}",
        typed_ast.errors
    );
}

#[test]
fn test_missing_nested_module() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use a;
        fn main() {}
        "#,
    );
    project.add_file("a/lib.som", "use nonexistent; fn a_func() {}");

    match project.load() {
        Ok(_) => panic!("should fail with missing nested module"),
        Err(errors) => {
            assert!(!errors.program.is_empty(), "expected program errors");
            let has_not_found = errors.program.iter().any(|e| {
                matches!(e, ProgramError::ModuleNotFound { name, .. } if name == "nonexistent")
            });
            assert!(has_not_found, "expected ModuleNotFound for 'nonexistent', got: {:?}", errors.program);
        }
    }
}

#[test]
fn test_module_with_constants() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use consts;
        fn main() -> i32 { consts::pi_approx() }
        "#,
    );
    project.add_file(
        "consts/lib.som",
        r#"
        fn pi_approx() -> i32 { 3 }
        fn zero() -> i32 { 0 }
        fn one() -> i32 { 1 }
        "#,
    );

    let ast = project.load().expect("should load");
    assert!(ast.func_registry.contains_key("consts::pi_approx"));
    assert!(ast.func_registry.contains_key("consts::zero"));
    assert!(ast.func_registry.contains_key("consts::one"));
}

#[test]
fn test_three_modules_independent() {
    let project = TestProject::new();
    project.add_file(
        "main.som",
        r#"
        use alpha;
        use beta;
        use gamma;
        fn main() {}
        "#,
    );
    project.add_file("alpha/lib.som", "fn a_fn() -> i32 { 1 }");
    project.add_file("beta/lib.som", "fn b_fn() -> i32 { 2 }");
    project.add_file("gamma/lib.som", "fn g_fn() -> i32 { 3 }");

    let ast = project.load().expect("should load");
    assert_eq!(ast.mods.len(), 4); // root + alpha + beta + gamma
    assert!(ast.func_registry.contains_key("alpha::a_fn"));
    assert!(ast.func_registry.contains_key("beta::b_fn"));
    assert!(ast.func_registry.contains_key("gamma::g_fn"));
}

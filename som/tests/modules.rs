use som::{LoadErrors, ProgramError, ProgramLoader};
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

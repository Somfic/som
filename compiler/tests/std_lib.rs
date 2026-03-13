//! End-to-end tests for the bundled standard library.

use som::{BorrowChecker, Codegen, Linker, ProgramLoader, Runner, TypeInferencer};
use std::fs;
use target_lexicon::Triple;
use tempfile::{NamedTempFile, TempDir};

/// Helper to create a test project, compile it, and run it
fn compile_and_run_project(files: &[(&str, &str)]) -> i32 {
    let dir = TempDir::new().unwrap();

    for (path, content) in files {
        let full_path = dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full_path, content).unwrap();
    }

    let loader = ProgramLoader::new(dir.path().to_path_buf());
    let ast = loader.load_project().expect("should load project");

    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    if !typed_ast.errors.is_empty() {
        panic!("Type errors: {:?}", typed_ast.errors);
    }

    let mut borrow_checker = BorrowChecker::new(&typed_ast);
    let borrow_errors = borrow_checker.check_program();
    if !borrow_errors.is_empty() {
        panic!("Borrow errors: {:?}", borrow_errors);
    }

    let codegen = Codegen::new(&typed_ast, Triple::host()).expect("codegen should work");
    let product = codegen.compile().expect("compile should work");

    let (libraries, needs_libc) = typed_ast.ast.get_extern_libraries();
    let library_paths = if cfg!(target_arch = "aarch64") {
        vec!["/opt/homebrew/lib".to_string()]
    } else {
        vec!["/usr/local/lib".to_string()]
    };

    let temp_file = NamedTempFile::new().unwrap();
    let temp_path = temp_file.into_temp_path();
    let linker = Linker::new(temp_path.to_str().unwrap())
        .with_libraries(libraries, needs_libc)
        .with_library_paths(library_paths);

    let executable = linker.link_object(product).expect("link should work");
    let runner = Runner::new(executable);
    let status = runner.run().expect("run should work");

    status.code().unwrap()
}

// ============================================================================
// Basic std functionality
// ============================================================================

#[test]
fn test_std_println() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("Hello from test!");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_malloc_free() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let p: * = std::malloc(100);
            std::free(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_exit_zero() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::exit(0);
            42
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_exit_nonzero() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::exit(7);
            0
        }
        "#,
    )]);
    assert_eq!(code, 7);
}

// ============================================================================
// Pointer type tests
// ============================================================================

#[test]
fn test_pointer_as_return_type() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn alloc_some() -> * {
            std::malloc(64)
        }
        fn main() -> i32 {
            let p: * = alloc_some();
            std::free(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_pointer_as_parameter() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn free_it(p: *) {
            std::free(p);
        }
        fn main() -> i32 {
            let p: * = std::malloc(64);
            free_it(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_multiple_allocations() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let a: * = std::malloc(10);
            let b: * = std::malloc(20);
            let c: * = std::malloc(30);
            std::free(c);
            std::free(b);
            std::free(a);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

// ============================================================================
// Module internal calls
// ============================================================================

#[test]
fn test_module_calls_own_extern() {
    // std::println internally calls puts - this should work
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("test1");
            std::println("test2");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_user_module_with_extern() {
    let code = compile_and_run_project(&[
        (
            "main.som",
            r#"
            use mylib;
            fn main() -> i32 {
                mylib::greet();
                0
            }
            "#,
        ),
        (
            "mylib/lib.som",
            r#"
            extern {
                fn puts(s: &str);
            }
            fn greet() {
                puts("Hello from mylib!");
            }
            "#,
        ),
    ]);
    assert_eq!(code, 0);
}

// ============================================================================
// Combined std usage
// ============================================================================

#[test]
fn test_full_std_usage() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("Starting...");
            let p: * = std::malloc(256);
            std::println("Allocated memory");
            std::free(p);
            std::println("Freed memory");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

// ============================================================================
// Additional std library tests
// ============================================================================

#[test]
fn test_std_println_multiple_lines() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("line one");
            std::println("line two");
            std::println("line three");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_println_empty_string() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_malloc_free_in_function() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn alloc_and_free() -> i32 {
            let p: * = std::malloc(128);
            std::free(p);
            0
        }
        fn main() -> i32 {
            alloc_and_free()
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_exit_large_code() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::exit(42);
            0
        }
        "#,
    )]);
    assert_eq!(code, 42);
}

#[test]
fn test_std_exit_in_conditional() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            if true {
                std::exit(5);
            }
            0
        }
        "#,
    )]);
    assert_eq!(code, 5);
}

#[test]
fn test_std_malloc_in_loop() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let mut i = 0;
            while i < 3 {
                let p: * = std::malloc(64);
                std::free(p);
                i = i + 1;
            }
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_combined_println_and_exit() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::println("about to exit");
            std::exit(7);
            0
        }
        "#,
    )]);
    assert_eq!(code, 7);
}

#[test]
fn test_std_combined_malloc_println_exit() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let p: * = std::malloc(100);
            std::println("allocated");
            std::free(p);
            std::println("freed");
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_user_module_calls_std() {
    let code = compile_and_run_project(&[
        (
            "main.som",
            r#"
            use mymod;
            fn main() -> i32 {
                mymod::do_print();
                0
            }
            "#,
        ),
        (
            "mymod/lib.som",
            r#"
            use std;
            fn do_print() {
                std::println("from user module");
            }
            "#,
        ),
    ]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_with_structs() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        struct Config { size: i32 }
        fn main() -> i32 {
            let c = Config { size: 64 };
            let p: * = std::malloc(c.size);
            std::free(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_println_in_while() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let mut i = 0;
            while i < 3 {
                std::println("loop");
                i = i + 1;
            }
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_multiple_malloc_sizes() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let a: * = std::malloc(8);
            let b: * = std::malloc(64);
            let c: * = std::malloc(512);
            std::free(a);
            std::free(b);
            std::free(c);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_exit_zero_explicit() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            std::exit(0);
            99
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_free_null_like() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn main() -> i32 {
            let p: * = std::malloc(16);
            std::free(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

#[test]
fn test_std_with_function_composition() {
    let code = compile_and_run_project(&[(
        "main.som",
        r#"
        use std;
        fn allocate(size: i32) -> * {
            std::malloc(size)
        }
        fn deallocate(p: *) {
            std::free(p);
        }
        fn main() -> i32 {
            let p: * = allocate(256);
            std::println("allocated via helper");
            deallocate(p);
            0
        }
        "#,
    )]);
    assert_eq!(code, 0);
}

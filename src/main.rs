use som::{BorrowChecker, Codegen, Diagnostic, Linker, Runner, Source, TypeInferencer, parser};
use target_lexicon::Triple;

use std::sync::Arc;

fn main() {
    let source_text = r#"
    extern {
        fn printf(s: &str, num: i32) -> i32;
    }

    fn main() {
        let a = 1 + 1 + 1;
        let b = 2 + 2 + 2;
        let c = a + b;
        printf("The result is: %d\n", c);
    }
    "#;

    let mut diagnostics: Vec<Diagnostic> = vec![];

    let source = Arc::new(Source::from_raw(source_text));
    let (ast, parse_errors) = parser::parse(source.clone());

    for error in &parse_errors {
        diagnostics.push(error.to_diagnostic());
    }

    let inferencer = TypeInferencer::new();
    let typed_ast = inferencer.check_program(ast);
    for error in &typed_ast.errors {
        diagnostics.push(error.to_diagnostic(&typed_ast.ast));
    }

    let mut borrow_checker = BorrowChecker::new(&typed_ast);
    for error in &borrow_checker.check_program() {
        diagnostics.push(error.to_diagnostic(&typed_ast));
    }

    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            println!("{}\n", diagnostic);
        }
        std::process::exit(1);
    }

    let codegen = match Codegen::new(&typed_ast, Triple::host()) {
        Ok(cg) => cg,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let product = match codegen.compile() {
        Ok(p) => p,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let (libraries, needs_libc) = typed_ast.ast.get_extern_libraries();

    // Add common library search paths (Homebrew)
    let library_paths = if cfg!(target_arch = "aarch64") {
        vec!["/opt/homebrew/lib".to_string()]
    } else {
        vec!["/usr/local/lib".to_string()]
    };

    let linker = Linker::new("som")
        .with_libraries(libraries, needs_libc)
        .with_library_paths(library_paths);
    let executable = match linker.link_object(product) {
        Ok(exe) => exe,
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    };

    let runner = Runner::new(executable);
    match runner.run() {
        Ok(status) => {
            println!("exited with {}", status.code().unwrap_or(0));
            std::process::exit(status.code().unwrap_or(0));
        }
        Err(diagnostic) => {
            println!("{}\n", diagnostic);
            std::process::exit(1);
        }
    }
}

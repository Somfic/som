use som::{
    BorrowChecker, Codegen, Diagnostic, Linker, ProgramLoader, Runner, Source, TypeInferencer,
    parser,
};
use target_lexicon::Triple;

use std::sync::Arc;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        eprintln!("Usage: som <source_file.som>");
        std::process::exit(1);
    }

    let source_dir = std::path::Path::new(&args[1]);
    if !source_dir.exists() {
        eprintln!("Error: file {:?} does not exist", source_dir);
        std::process::exit(1);
    }

    let mut diagnostics = Vec::new();

    let loader = ProgramLoader::new(source_dir.to_path_buf());
    let ast = match loader.load_project() {
        Ok(ast) => ast,
        Err(errors) => {
            for error in errors {
                // need to implement Display or to_diagnostic for ProgramError
                eprintln!("{}", error.to_diagnostic());
            }
            std::process::exit(1);
        }
    };

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

use som::{BorrowChecker, Codegen, Diagnostic, Linker, Runner, Source, TypeInferencer, parser};
use target_lexicon::Triple;

use std::sync::Arc;

fn main() {
    let source_text = r#"
    extern "raylib" {
        fn InitWindow(width: i32, height: i32, title: &str);
        fn SetTargetFPS(fps: i32);
        fn WindowShouldClose() -> bool;
        fn BeginDrawing();
        fn EndDrawing();
        fn CloseWindow();
        fn ClearBackground(color: Color);
        fn DrawCircle(x: i32, y: i32, radius: f32, color: Color);
        fn DrawRectangle(x: i32, y: i32, width: i32, height: i32, color: Color);
    }

    struct Color {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    }

    fn main() -> i32 {
        InitWindow(800, 600, "Hello, world!");
        SetTargetFPS(60);

        while !WindowShouldClose() {
          BeginDrawing();
          ClearBackground(Color { r: 30, g: 30, b: 30, a: 255 });
          DrawCircle(400, 300, 100.0, Color { r: 255, g: 100, b: 100, a: 255 });
          EndDrawing();
      }

        CloseWindow();

        0
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

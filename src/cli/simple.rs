use crate::{lowering::Lowering, prelude::*};

/// Run compilation without process tree visualization
pub fn run(source: miette::NamedSource<String>) -> i64 {
    // Helper function to handle panics and convert them to errors
    fn handle_panic(
        panic: Box<dyn std::any::Any + Send>,
        stage: &str,
        source: &miette::NamedSource<String>,
    ) {
        let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
            msg.clone()
        } else if let Some(msg) = panic.downcast_ref::<&str>() {
            msg.to_string()
        } else {
            format!("Unknown {} error", stage)
        };

        let error = Error::Compiler(CompilerError::CodeGenerationFailed {
            span: Span::default(),
            help: format!("{} failed: {}", stage, panic_message),
        });

        eprintln!(
            "{:?}",
            miette::miette!(error).with_source_code(source.clone())
        );
        std::process::exit(1);
    }

    // Stage 1: Lexing - catch panics
    let lexer = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Lexer::new(source.inner().as_str())
    })) {
        Ok(lexer) => lexer,
        Err(panic) => {
            handle_panic(panic, "Lexing", &source);
            unreachable!()
        }
    };

    // Stage 2: Parsing - catch panics
    let parsed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut parser = Parser::new(lexer);
        parser.parse()
    })) {
        Ok(Ok(parsed)) => parsed,
        Ok(Err(errors)) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Parsing", &source);
            unreachable!()
        }
    };

    // Stage 3: Type checking - catch panics
    let type_checked = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut type_checker = TypeChecker::new();
        type_checker.check(&parsed)
    })) {
        Ok(Ok(typed_statement)) => typed_statement,
        Ok(Err(errors)) => {
            for error in errors {
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
            }
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Type checking", &source);
            unreachable!()
        }
    };

    // Stage 4: Lowering - catch panics
    let type_checked_span = type_checked.span; // Save span before move
    let (lowered, metadata) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut lowering = Lowering::new();
        let lowered = lowering.lower(type_checked);
        (lowered, lowering.metadata)
    })) {
        Ok(result) => result,
        Err(panic) => {
            handle_panic(panic, "Lowering", &source);
            unreachable!()
        }
    };

    // Stage 5: Code generation - catch panics
    let (compiled, return_type) =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut compiler = Compiler::new(metadata);
            compiler.compile(&lowered)
        })) {
            Ok(Ok(result)) => result, // Successful compilation
            Ok(Err(error)) => {
                // Regular error
                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
                std::process::exit(1);
            }
            Err(panic) => {
                // Panic occurred during compilation
                let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
                    msg.clone()
                } else if let Some(msg) = panic.downcast_ref::<&str>() {
                    msg.to_string()
                } else {
                    "Unknown compilation error".to_string()
                };

                let error = Error::Compiler(CompilerError::CodeGenerationFailed {
                    span: type_checked_span,
                    help: format!("Code generation failed: {}", panic_message),
                });

                eprintln!(
                    "{:?}",
                    miette::miette!(error).with_source_code(source.clone())
                );
                std::process::exit(1);
            }
        };

    // Stage 6: Execution - catch panics
    let return_value = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let runner = Runner::new();
        runner.run(compiled, &return_type)
    })) {
        Ok(Ok(return_value)) => return_value,
        Ok(Err(error)) => {
            eprintln!("Runtime error: {:?}", error);
            std::process::exit(1);
        }
        Err(panic) => {
            handle_panic(panic, "Execution", &source);
            unreachable!()
        }
    };

    return_value
}

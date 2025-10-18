use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use colored::Colorize;

use crate::lowering::Lowering;
use crate::tui::{format_process_name, Process, ProcessState};
use crate::{prelude::*, tui};

use super::compilation_result;

pub fn run_with_process_tree(
    source: miette::NamedSource<String>,
    source_path: Option<PathBuf>,
) -> Option<i64> {
    use compilation_result::COMPILED_CODE;

    // Reset the compiled code storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.code = None;
        code_storage.return_type = None;
    }

    let now = SystemTime::now();

    // Create the process tree for compilation stages
    let process_tree = Arc::new(Mutex::new(Process {
        name: format!("{}", source.name()),
        state: ProcessState::Running,
        note: Some("lexing".to_string()),
        started_at: now,
        completed_at: None,
        children: vec![],
    }));

    // Clone for the compilation thread
    let tree_clone = Arc::clone(&process_tree);
    let source_clone = source.clone();
    let source_path_clone = source_path.clone();

    // Run compilation in a separate thread
    let compilation_result = Arc::new(Mutex::new(None));
    let result_clone = Arc::clone(&compilation_result);

    let _compilation_thread = thread::spawn(move || {
        let result = run_compilation_stages(source_clone, source_path_clone, tree_clone);
        *result_clone.lock().unwrap() = Some(result);
    });

    // Display the animated tree
    loop {
        {
            let tree = process_tree.lock().unwrap();
            crate::tui::draw_process_tree_animated(&tree);
        }

        // Check if compilation is done
        let compilation_guard = compilation_result.lock().unwrap();
        if let Some(result) = compilation_guard.as_ref() {
            match result {
                Ok(_) => {
                    drop(compilation_guard); // Release the lock

                    // Update main process to completed
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Completed;
                        tree.note = None;
                        tree.completed_at = Some(SystemTime::now());
                    }

                    // Final draw of the process tree
                    {
                        let tree = process_tree.lock().unwrap();
                        crate::tui::draw_process_tree_animated(&tree);
                    }

                    eprintln!("");

                    // Print that the compilation completed
                    tui::print_success(format!(
                        "compilation {}",
                        format_process_name("succeeded", &ProcessState::Completed).bright_green()
                    ));

                    eprintln!("");

                    // Retrieve the compiled code from global storage
                    let (code_ptr, return_type) = {
                        let code_storage = COMPILED_CODE.lock().unwrap();
                        match (&code_storage.code, &code_storage.return_type) {
                            (Some(ptr), Some(rt)) => (*ptr, rt.clone()),
                            _ => {
                                tui::print_error("No compiled code available!".to_string());
                                return None;
                            }
                        }
                    };

                    // Execute the compiled code - catch panics
                    let return_value =
                        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let runner = Runner::new();
                            runner.run(code_ptr, &return_type)
                        })) {
                            Ok(Ok(return_value)) => return_value,
                            Ok(Err(error)) => {
                                tui::print_error(format!("Runtime error: {:?}", error));
                                return None;
                            }
                            Err(panic) => {
                                let panic_message =
                                    if let Some(msg) = panic.downcast_ref::<String>() {
                                        msg.clone()
                                    } else if let Some(msg) = panic.downcast_ref::<&str>() {
                                        msg.to_string()
                                    } else {
                                        "Unknown runtime error".to_string()
                                    };
                                tui::print_error(format!(
                                    "Runtime execution failed: {}",
                                    panic_message
                                ));
                                return None;
                            }
                        };

                    println!("  ⚡ Result: {}", return_value);

                    return Some(return_value);
                }
                Err(error_reports) => {
                    // Update main process to error
                    {
                        let mut tree = process_tree.lock().unwrap();
                        tree.state = ProcessState::Error;
                        tree.note = format!(
                            "{} {}",
                            error_reports.len(),
                            if error_reports.len() == 1 {
                                "error"
                            } else {
                                "errors"
                            }
                        )
                        .into();
                        tree.completed_at = Some(SystemTime::now());
                    }

                    // Show error state for a moment
                    {
                        let tree = process_tree.lock().unwrap();
                        crate::tui::draw_process_tree_animated(&tree);
                    }

                    eprintln!("");

                    // Print miette errors with proper formatting
                    for error_report in error_reports {
                        eprintln!("{:?}", error_report);
                    }

                    // Print that the compilation failed with x errors
                    tui::print_error(format!(
                        "compilation {} with {} {}",
                        format_process_name("failed", &ProcessState::Error).bright_green(),
                        error_reports.len(),
                        if error_reports.len() == 1 {
                            "error"
                        } else {
                            "errors"
                        }
                    ));

                    eprintln!("");

                    return None;
                }
            }
        } else {
            drop(compilation_guard); // Release the lock
        }

        // 10fps for animations
        thread::sleep(Duration::from_millis(100));
    }
}

/// Run compilation stages with process tree updates
/// Instead of returning the raw pointer, we store it in a global and return success/failure
fn run_compilation_stages(
    source: miette::NamedSource<String>,
    source_path: Option<PathBuf>,
    process_tree: Arc<Mutex<Process>>,
) -> std::result::Result<(), Vec<miette::Report>> {
    use crate::module::ModuleLoader;
    use compilation_result::COMPILED_CODE;

    fn update_stage_note(tree: &Arc<Mutex<Process>>, stage_name: &str) {
        let mut tree = tree.lock().unwrap();
        tree.note = Some(stage_name.to_string());
    }

    // Helper function to handle panics and convert them to error reports
    fn handle_panic(
        panic: Box<dyn std::any::Any + Send>,
        stage: &str,
        source: &miette::NamedSource<String>,
    ) -> Vec<miette::Report> {
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

        let report = miette::miette!(error).with_source_code(source.clone());
        vec![report]
    }

    // Stage 1: Parse and check for imports
    update_stage_note(&process_tree, "parsing & loading modules");

    // Use the actual source path if provided, otherwise create a temporary file
    let file_path = if let Some(path) = source_path {
        path
    } else {
        let temp_file_path = std::env::temp_dir().join(format!("{}.som", source.name()));
        std::fs::write(&temp_file_path, source.inner()).map_err(|e| {
            vec![
                miette::miette!(Error::Compiler(CompilerError::CodeGenerationFailed {
                    span: Span::default(),
                    help: format!("Failed to write temporary file: {}", e),
                }))
                .with_source_code(source.clone()),
            ]
        })?;
        temp_file_path
    };

    let mut module_loader = ModuleLoader::new();
    match module_loader.load_module(&file_path) {
        Ok(()) => {}
        Err(error) => {
            // Check if this is a ModuleError with source information
            let report = if let Error::Module(ref module_error) = error {
                match module_error {
                    crate::module::ModuleError::ParseError {
                        path,
                        source: module_source,
                        ..
                    } => {
                        let module_name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown");
                        let named_source =
                            miette::NamedSource::new(module_name, module_source.clone());
                        miette::miette!(error).with_source_code(named_source)
                    }
                }
            } else {
                miette::miette!(error).with_source_code(source.clone())
            };
            return Err(vec![report]);
        }
    }

    // Get modules in dependency order
    let modules = match module_loader.modules_in_dependency_order() {
        Ok(modules) => modules,
        Err(error) => {
            let report = miette::miette!(error).with_source_code(source.clone());
            return Err(vec![report]);
        }
    };

    // Stage 2: Type check all modules in dependency order
    update_stage_note(&process_tree, "type checking");

    let mut typed_modules: HashMap<PathBuf, TypedStatement> = HashMap::new();
    let mut module_types: HashMap<PathBuf, HashMap<String, Type>> = HashMap::new();

    // Get the main module to filter it out from children
    let main_module_path = module_loader.get_module(&file_path).unwrap().path.clone();
    let main_module = module_loader.get_module(&file_path).unwrap();

    // Build hierarchical tree of module processes
    // Map from module path to (process, child_indices)
    let mut module_processes: HashMap<PathBuf, (Process, Vec<usize>)> = HashMap::new();

    // Helper function to recursively build the module tree
    fn build_module_tree(
        module: &crate::module::Module,
        module_loader: &crate::module::ModuleLoader,
        module_processes: &mut HashMap<PathBuf, (Process, Vec<usize>)>,
        main_module_path: &PathBuf,
    ) {
        // Skip if already processed
        if module_processes.contains_key(&module.path) {
            return;
        }

        // Create process for this module
        let mut process = Process {
            name: module.name.clone(),
            state: ProcessState::Running,
            note: Some("type checking".to_string()),
            started_at: std::time::SystemTime::now(),
            completed_at: None,
            children: vec![],
        };

        // Process imports and add them as children
        let parent_dir = module.path.parent().unwrap_or(Path::new("."));
        for import_path_str in &module.imports {
            if let Ok(resolved_path) =
                module_loader.canonicalize_path(&parent_dir.join(import_path_str))
            {
                if let Some(imported_module) = module_loader.get_module(&resolved_path) {
                    // Recursively build the imported module's tree
                    build_module_tree(
                        imported_module,
                        module_loader,
                        module_processes,
                        main_module_path,
                    );

                    // Add the imported module's process as a child
                    if let Some((imported_process, _)) =
                        module_processes.get(&resolved_path).cloned()
                    {
                        process.children.push(imported_process);
                    }
                }
            }
        }

        module_processes.insert(module.path.clone(), (process, vec![]));
    }

    // Build tree starting from modules directly imported by main
    for import_path_str in &main_module.imports {
        let parent_dir = main_module.path.parent().unwrap_or(Path::new("."));
        if let Ok(resolved_path) =
            module_loader.canonicalize_path(&parent_dir.join(import_path_str))
        {
            if let Some(imported_module) = module_loader.get_module(&resolved_path) {
                build_module_tree(
                    imported_module,
                    &module_loader,
                    &mut module_processes,
                    &main_module_path,
                );
            }
        }
    }

    // Add top-level imported modules to the main process tree
    for import_path_str in &main_module.imports {
        let parent_dir = main_module.path.parent().unwrap_or(Path::new("."));
        if let Ok(resolved_path) =
            module_loader.canonicalize_path(&parent_dir.join(import_path_str))
        {
            if let Some((process, _)) = module_processes.get(&resolved_path).cloned() {
                let mut tree = process_tree.lock().unwrap();
                tree.children.push(process);
            }
        }
    }

    // Now type check all modules
    for module in &modules {
        // Get types from directly imported modules
        let parent_dir = module.path.parent().unwrap_or(Path::new("."));
        let mut imported_types = Vec::new();

        for import_path_str in &module.imports {
            // Resolve the import path
            let resolved_path =
                match module_loader.canonicalize_path(&parent_dir.join(import_path_str)) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

            // Get the types from the imported module (which was already type-checked)
            if let Some(types_map) = module_types.get(&resolved_path) {
                for (name, type_) in types_map {
                    imported_types.push((name.clone(), type_.clone()));
                }
            }
        }

        // Type check this module

        let mut type_checker = TypeChecker::new();
        let type_checked = match type_checker.check_with_imports(&module.parsed, &imported_types) {
            Ok(typed) => typed,
            Err(errors) => {
                let mut error_messages = Vec::new();
                for error in errors {
                    // Only show errors from the current module being checked
                    // to avoid confusion with cross-file span offsets
                    let source = miette::NamedSource::new(&module.name, module.source.clone());
                    let report = miette::miette!(error).with_source_code(source);
                    error_messages.push(report);
                }
                return Err(error_messages);
            }
        };

        // Extract the types of declarations from this module for use by other modules
        let mut this_module_types = HashMap::new();
        if let StatementValue::VariableDeclaration(var_decl) = &type_checked.value {
            if let TypedExpressionValue::Function(func) = &var_decl.value.value {
                if let TypedExpressionValue::Block(block) = &func.body.value {
                    for stmt in &block.statements {
                        match &stmt.value {
                            StatementValue::VariableDeclaration(var) => {
                                this_module_types.insert(
                                    var.identifier.name.to_string(),
                                    var.value.type_.clone(),
                                );
                            }
                            StatementValue::ExternDeclaration(ext) => {
                                let type_ = TypeValue::Function(FunctionType {
                                    parameters: ext.signature.parameters.clone(),
                                    return_type: Box::new(ext.signature.return_type.clone()),
                                    span: ext.signature.span,
                                })
                                .with_span(ext.identifier.span);
                                this_module_types.insert(ext.identifier.name.to_string(), type_);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        module_types.insert(module.path.clone(), this_module_types);
        typed_modules.insert(module.path.clone(), type_checked);

        // Mark this module as completed in the tree (recursively search for it)
        if module.path != main_module_path {
            fn mark_completed_recursive(processes: &mut [Process], module_name: &str) -> bool {
                for process in processes {
                    if process.name == module_name {
                        process.state = ProcessState::Completed;
                        process.completed_at = Some(std::time::SystemTime::now());
                        return true;
                    }
                    if mark_completed_recursive(&mut process.children, module_name) {
                        return true;
                    }
                }
                false
            }

            let mut tree = process_tree.lock().unwrap();
            mark_completed_recursive(&mut tree.children, &module.name);
        }
    }

    // Stage 3: Lowering
    update_stage_note(&process_tree, "lowering");

    // Run lowering pass on all modules to collect metadata
    let mut lowering = Lowering::new();
    for (_path, typed_module) in &typed_modules {
        lowering.lower(typed_module.clone());
    }
    let metadata = lowering.metadata;

    // Stage 4: Code generation
    update_stage_note(&process_tree, "code generation");

    // Rebuild the hierarchical tree for code generation phase
    {
        let mut tree = process_tree.lock().unwrap();
        tree.children.clear();
    }

    // Reset module_processes for code generation
    let mut module_processes: HashMap<PathBuf, (Process, Vec<usize>)> = HashMap::new();

    // Helper function to recursively build the module tree for code generation
    fn build_codegen_tree(
        module: &crate::module::Module,
        module_loader: &crate::module::ModuleLoader,
        module_processes: &mut HashMap<PathBuf, (Process, Vec<usize>)>,
        main_module_path: &PathBuf,
    ) {
        if module_processes.contains_key(&module.path) {
            return;
        }

        let mut process = Process {
            name: module.name.clone(),
            state: ProcessState::Running,
            note: Some("compiling".to_string()),
            started_at: std::time::SystemTime::now(),
            completed_at: None,
            children: vec![],
        };

        let parent_dir = module.path.parent().unwrap_or(Path::new("."));
        for import_path_str in &module.imports {
            if let Ok(resolved_path) =
                module_loader.canonicalize_path(&parent_dir.join(import_path_str))
            {
                if let Some(imported_module) = module_loader.get_module(&resolved_path) {
                    build_codegen_tree(
                        imported_module,
                        module_loader,
                        module_processes,
                        main_module_path,
                    );
                    if let Some((imported_process, _)) =
                        module_processes.get(&resolved_path).cloned()
                    {
                        process.children.push(imported_process);
                    }
                }
            }
        }

        module_processes.insert(module.path.clone(), (process, vec![]));
    }

    // Build tree for code generation
    for import_path_str in &main_module.imports {
        let parent_dir = main_module.path.parent().unwrap_or(Path::new("."));
        if let Ok(resolved_path) =
            module_loader.canonicalize_path(&parent_dir.join(import_path_str))
        {
            if let Some(imported_module) = module_loader.get_module(&resolved_path) {
                build_codegen_tree(
                    imported_module,
                    &module_loader,
                    &mut module_processes,
                    &main_module_path,
                );
            }
        }
    }

    // Add top-level imported modules to the main process tree
    for import_path_str in &main_module.imports {
        let parent_dir = main_module.path.parent().unwrap_or(Path::new("."));
        if let Ok(resolved_path) =
            module_loader.canonicalize_path(&parent_dir.join(import_path_str))
        {
            if let Some((process, _)) = module_processes.get(&resolved_path).cloned() {
                let mut tree = process_tree.lock().unwrap();
                tree.children.push(process);
            }
        }
    }

    // Get the main module (the entry point module)
    let main_module = module_loader.get_module(&file_path).unwrap();
    let main_typed = typed_modules.get(&main_module.path).unwrap();

    let (compiled, return_type) = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || {
            let mut compiler = Compiler::new(metadata);

            // Compile all dependencies first (all modules except the main one)
            for module in &modules {
                if module.path == main_module.path {
                    continue; // Skip main module for now
                }

                let typed_module = typed_modules.get(&module.path).unwrap();

                // Compile each declaration in the module
                if let StatementValue::VariableDeclaration(var_decl) = &typed_module.value {
                    if let TypedExpressionValue::Function(func) = &var_decl.value.value {
                        if let TypedExpressionValue::Block(block) = &func.body.value {
                            for stmt in &block.statements {
                                if let StatementValue::VariableDeclaration(decl) = &stmt.value {
                                    if let TypedExpressionValue::Function(_) = &decl.value.value {
                                        // Create environment for this function with all current declarations
                                        let mut func_env = crate::compiler::Environment::new(
                                            compiler.declarations.clone(),
                                        );

                                        // This is a function declaration - compile it
                                        let (func_id, _) = crate::expressions::function::compile(
                                            &mut compiler,
                                            &decl.value,
                                            &mut func_env,
                                        );
                                        compiler.declarations.insert(
                                        decl.identifier.name.to_string(),
                                        crate::compiler::environment::DeclarationValue::Function(func_id),
                                    );
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Now compile the main module
            compiler.compile(main_typed)
        },
    )) {
        Ok(Ok(result)) => {
            // Mark all modules as compiled (recursively)
            {
                fn mark_all_completed_recursive(processes: &mut [Process]) {
                    for process in processes {
                        process.state = ProcessState::Completed;
                        process.completed_at = Some(std::time::SystemTime::now());
                        mark_all_completed_recursive(&mut process.children);
                    }
                }

                let mut tree = process_tree.lock().unwrap();
                mark_all_completed_recursive(&mut tree.children);
            }
            result
        }
        Ok(Err(error)) => {
            let report = miette::miette!(error).with_source_code(source.clone());
            return Err(vec![report]);
        }
        Err(panic) => {
            let panic_message = if let Some(msg) = panic.downcast_ref::<String>() {
                msg.clone()
            } else if let Some(msg) = panic.downcast_ref::<&str>() {
                msg.to_string()
            } else {
                "Unknown compilation error".to_string()
            };

            let error = Error::Compiler(CompilerError::CodeGenerationFailed {
                span: main_typed.span,
                help: format!("Code generation failed: {}", panic_message),
            });

            let report = miette::miette!(error).with_source_code(source.clone());
            return Err(vec![report]);
        }
    };

    // Store the compiled code in our global storage
    {
        let mut code_storage = COMPILED_CODE.lock().unwrap();
        code_storage.set_code(compiled, return_type);
    }

    // Return success
    Ok(())
}

use crate::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Module-specific errors that carry source information
#[derive(Debug, Clone)]
pub enum ModuleError {
    ParseError {
        path: PathBuf,
        source: String,
        error: Box<Error>,
    },
}

impl std::fmt::Display for ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleError::ParseError { error, .. } => {
                write!(f, "{}", error)
            }
        }
    }
}

impl std::error::Error for ModuleError {}

impl miette::Diagnostic for ModuleError {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        match self {
            ModuleError::ParseError { error, .. } => {
                if let Error::Parser(e) = error.as_ref() {
                    miette::Diagnostic::code(e)
                } else {
                    None
                }
            }
        }
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(miette::Severity::Error)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        match self {
            ModuleError::ParseError { error, .. } => {
                if let Error::Parser(e) = error.as_ref() {
                    miette::Diagnostic::help(e)
                } else {
                    None
                }
            }
        }
    }

    fn url<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        None
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        match self {
            ModuleError::ParseError { error, .. } => {
                if let Error::Parser(e) = error.as_ref() {
                    miette::Diagnostic::labels(e)
                } else {
                    None
                }
            }
        }
    }
}

/// Represents a top-level declaration in a module
#[derive(Debug, Clone)]
pub enum Declaration {
    Variable(VariableDeclarationStatement<Expression>),
    Extern(ExternDeclarationStatement),
    Type(TypeDeclarationStatement),
}

impl Declaration {
    /// Get the identifier name of this declaration
    pub fn identifier(&self) -> &Identifier {
        match self {
            Declaration::Variable(var) => &var.identifier,
            Declaration::Extern(ext) => &ext.identifier,
            Declaration::Type(ty) => &ty.identifier,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Module {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
    pub parsed: Statement,
    pub imports: Vec<String>,
    pub declarations: Vec<Declaration>,
}

pub struct ModuleLoader {
    modules: HashMap<PathBuf, Module>,
    loading_stack: Vec<PathBuf>,
}

impl ModuleLoader {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
            loading_stack: Vec::new(),
        }
    }

    /// Load a module and all its dependencies recursively
    pub fn load_module(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let canonical_path = self.canonicalize_path(path)?;

        // Check if already loaded
        if self.modules.contains_key(&canonical_path) {
            return Ok(());
        }

        // Check for circular dependency
        if self.loading_stack.contains(&canonical_path) {
            return Err(Error::Compiler(CompilerError::CodeGenerationFailed {
                span: Span::default(),
                help: format!(
                    "Circular dependency detected: {} -> {}",
                    self.loading_stack
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" -> "),
                    canonical_path.display()
                ),
            }));
        }

        // Push to loading stack
        self.loading_stack.push(canonical_path.clone());

        // Read the source file
        let source = std::fs::read_to_string(&canonical_path).map_err(|e| {
            Error::Compiler(CompilerError::CodeGenerationFailed {
                span: Span::default(),
                help: format!(
                    "Failed to read module '{}': {}",
                    canonical_path.display(),
                    e
                ),
            })
        })?;

        // Parse the module
        let lexer = Lexer::new(&source);
        let mut parser = Parser::new(lexer);
        let parsed = parser.parse().map_err(|errors| {
            // Return a ModuleError that includes source information
            let error = errors.first().cloned().unwrap();
            Error::Module(ModuleError::ParseError {
                path: canonical_path.clone(),
                source: source.clone(),
                error: Box::new(error),
            })
        })?;

        // Extract imports from the parsed AST
        let imports = self.extract_imports(&parsed);

        // Extract declarations from the parsed AST
        let declarations = self.extract_declarations(&parsed);

        // Create the module
        let name = canonical_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let module = Module {
            name,
            path: canonical_path.clone(),
            source,
            parsed,
            imports: imports.clone(),
            declarations,
        };

        // Store the module
        self.modules.insert(canonical_path.clone(), module);

        // Pop from loading stack
        self.loading_stack.pop();

        // Recursively load dependencies
        let parent_dir = canonical_path.parent().unwrap_or(Path::new("."));
        for import_path in imports {
            let resolved_path = parent_dir.join(&import_path);
            self.load_module(&resolved_path)?;
        }

        Ok(())
    }

    /// Get a loaded module by path
    pub fn get_module(&self, path: impl AsRef<Path>) -> Option<&Module> {
        let canonical_path = self.canonicalize_path(path.as_ref()).ok()?;
        self.modules.get(&canonical_path)
    }

    /// Get all loaded modules
    pub fn modules(&self) -> &HashMap<PathBuf, Module> {
        &self.modules
    }

    /// Get the declarations that should be visible to a module (from its direct imports)
    pub fn get_imported_declarations(&self, module: &Module) -> Vec<Declaration> {
        let mut declarations = Vec::new();
        let parent_dir = module.path.parent().unwrap_or(Path::new("."));

        for import_path in &module.imports {
            let resolved_path = match self.canonicalize_path(&parent_dir.join(import_path)) {
                Ok(path) => path,
                Err(_) => continue,
            };

            if let Some(imported_module) = self.modules.get(&resolved_path) {
                declarations.extend(imported_module.declarations.clone());
            }
        }

        declarations
    }

    /// Extract import paths from a parsed statement
    fn extract_imports(&self, statement: &Statement) -> Vec<String> {
        let mut imports = Vec::new();

        // The main statement is a VariableDeclaration wrapping a Function
        // The function body is a Block containing statements
        if let StatementValue::VariableDeclaration(var_decl) = &statement.value {
            if let ExpressionValue::Function(func) = &var_decl.value.value {
                if let ExpressionValue::Block(block) = &func.body.value {
                    for stmt in &block.statements {
                        if let StatementValue::Import(import) = &stmt.value {
                            imports.push(import.path.clone());
                        }
                    }
                }
            }
        }

        imports
    }

    /// Extract top-level declarations from a parsed statement
    fn extract_declarations(&self, statement: &Statement) -> Vec<Declaration> {
        let mut declarations = Vec::new();

        // The main statement is a VariableDeclaration wrapping a Function
        // The function body is a Block containing statements
        if let StatementValue::VariableDeclaration(var_decl) = &statement.value {
            if let ExpressionValue::Function(func) = &var_decl.value.value {
                if let ExpressionValue::Block(block) = &func.body.value {
                    for stmt in &block.statements {
                        match &stmt.value {
                            StatementValue::VariableDeclaration(var_decl) => {
                                declarations.push(Declaration::Variable(var_decl.clone()));
                            }
                            StatementValue::ExternDeclaration(extern_decl) => {
                                declarations.push(Declaration::Extern(extern_decl.clone()));
                            }
                            StatementValue::TypeDeclaration(type_decl) => {
                                declarations.push(Declaration::Type(type_decl.clone()));
                            }
                            StatementValue::Import(_) => {
                                // Skip imports, they're already handled
                            }
                            StatementValue::Expression(_) => {
                                // Skip top-level expressions (they're not declarations)
                            }
                        }
                    }
                }
            }
        }

        declarations
    }

    /// Canonicalize a path to avoid duplicates
    pub fn canonicalize_path(&self, path: &Path) -> Result<PathBuf> {
        // Try to canonicalize, but if it fails (e.g., file doesn't exist yet),
        // just return the absolute path
        path.canonicalize().or_else(|_| {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .map_err(|e| {
                    Error::Compiler(CompilerError::CodeGenerationFailed {
                        span: Span::default(),
                        help: format!("Failed to resolve path '{}': {}", path.display(), e),
                    })
                })
        })
    }

    /// Get modules in dependency order (dependencies before dependents)
    pub fn modules_in_dependency_order(&self) -> Result<Vec<&Module>> {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut result = Vec::new();

        for module_path in self.modules.keys() {
            if !visited.contains(module_path) {
                self.visit_module(module_path, &mut visited, &mut stack, &mut result)?;
            }
        }

        // Don't reverse - visit_module already adds dependencies before dependents
        Ok(result)
    }

    fn visit_module<'a>(
        &'a self,
        path: &PathBuf,
        visited: &mut HashSet<PathBuf>,
        stack: &mut Vec<PathBuf>,
        result: &mut Vec<&'a Module>,
    ) -> Result<()> {
        if stack.contains(path) {
            return Err(Error::Compiler(CompilerError::CodeGenerationFailed {
                span: Span::default(),
                help: format!("Circular dependency detected at {}", path.display()),
            }));
        }

        if visited.contains(path) {
            return Ok(());
        }

        stack.push(path.clone());
        visited.insert(path.clone());

        let module = self.modules.get(path).unwrap();
        let parent_dir = path.parent().unwrap_or(Path::new("."));

        for import_path in &module.imports {
            let resolved_path = self.canonicalize_path(&parent_dir.join(import_path))?;
            self.visit_module(&resolved_path, visited, stack, result)?;
        }

        stack.pop();
        result.push(module);

        Ok(())
    }
}

use crate::{Ast, Expr, Func, Path, Span, Stmt, arena::Id};
use std::collections::HashMap;

mod error;
pub use error::*;

/// Unique identifier for any definition (function, variable, parameter)
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct DefId(pub u32);

/// Special DefId for unresolved names (prevents cascading errors)
pub const ERROR_DEF: DefId = DefId(u32::MAX);

/// What kind of definition this is
#[derive(Clone, Debug)]
pub enum DefKind {
    /// Top-level or nested function
    Function {
        func_id: Id<Func>,
        can_capture: bool,
    },
    /// Function parameter
    Parameter {
        func: DefId,   // Which function owns this
        index: usize,
    },
    /// Local variable from let binding
    Variable,
}

/// Information stored about each definition
#[derive(Clone, Debug)]
pub struct Definition {
    pub kind: DefKind,
    pub name: Box<str>,
    pub span: Span,
}

/// Kind of scope (affects visibility rules)
#[derive(Clone, Debug)]
pub enum RibKind {
    /// Module-level scope
    Module,
    /// Function scope
    /// - Blocks outer locals (unless can_capture is true)
    /// - Allows outer functions
    Function {
        def_id: DefId,
        can_capture: bool,
    },
    /// Block scope (transparent)
    Block,
}

/// A rib is a scope containing name bindings
pub struct Rib {
    pub bindings: HashMap<String, DefId>,
    pub kind: RibKind,
}

impl Rib {
    fn new(kind: RibKind) -> Self {
        Self {
            bindings: HashMap::new(),
            kind,
        }
    }
}

/// Result of name resolution
pub struct ResolvedAst {
    pub ast: Ast,
    /// All definitions in the program
    pub definitions: Vec<Definition>,
    /// ExprId -> DefId for variable references (Expr::Var)
    pub var_resolutions: HashMap<Id<Expr>, DefId>,
    /// FuncId -> Vec<DefId> of captured variables (for closures - future)
    pub captures: HashMap<Id<Func>, Vec<DefId>>,
    /// Errors encountered during resolution
    pub errors: Vec<ResolveError>,
}

/// The name resolver
pub struct NameResolver {
    /// Stack of ribs (scopes)
    ribs: Vec<Rib>,
    /// All definitions created
    definitions: Vec<Definition>,
    /// Next DefId to allocate
    next_def_id: u32,
    /// Resolution results
    var_resolutions: HashMap<Id<Expr>, DefId>,
    captures: HashMap<Id<Func>, Vec<DefId>>,
    /// Errors encountered
    errors: Vec<ResolveError>,
}

impl NameResolver {
    pub fn new() -> Self {
        Self {
            ribs: Vec::new(),
            definitions: Vec::new(),
            next_def_id: 0,
            var_resolutions: HashMap::new(),
            captures: HashMap::new(),
            errors: Vec::new(),
        }
    }

    /// Main entry point: resolve all names in the AST
    pub fn resolve(ast: Ast) -> ResolvedAst {
        let mut resolver = Self::new();

        // Phase 1: Collect top-level definitions
        resolver.collect_top_level(&ast);

        // Phase 2: Resolve function bodies
        resolver.resolve_functions(&ast);

        ResolvedAst {
            ast,
            definitions: resolver.definitions,
            var_resolutions: resolver.var_resolutions,
            captures: resolver.captures,
            errors: resolver.errors,
        }
    }

    fn push_rib(&mut self, kind: RibKind) {
        self.ribs.push(Rib::new(kind));
    }

    fn pop_rib(&mut self) {
        self.ribs.pop();
    }

    /// Create a new definition and return its DefId
    fn define(&mut self, name: &str, kind: DefKind, span: Span) -> DefId {
        let def_id = DefId(self.next_def_id);
        self.next_def_id += 1;

        self.definitions.push(Definition {
            kind,
            name: name.into(),
            span,
        });

        def_id
    }

    /// Phase 1: Collect all top-level function definitions
    fn collect_top_level(&mut self, ast: &Ast) {
        self.push_rib(RibKind::Module);

        // Register all functions (allows forward references)
        for (func_id, func) in ast.funcs.iter_with_ids() {
            let def_id = self.define(
                &func.name.value,
                DefKind::Function {
                    func_id,
                    can_capture: false, // Top-level functions cannot capture
                },
                ast.get_func_span(&func_id).clone(),
            );

            // Add to module rib
            self.ribs
                .last_mut()
                .unwrap()
                .bindings
                .insert(func.name.value.to_string(), def_id);
        }
    }

    /// Phase 2: Resolve all function bodies
    fn resolve_functions(&mut self, ast: &Ast) {
        for (func_id, _) in ast.funcs.iter_with_ids() {
            self.resolve_func(func_id, ast, None);
        }

        // Pop module rib
        self.pop_rib();
    }

    /// Resolve a single function
    fn resolve_func(&mut self, func_id: Id<Func>, ast: &Ast, _parent_func: Option<Id<Func>>) {
        let func = ast.funcs.get(&func_id);

        // Get function's DefId from outer scope
        let func_def_id = self
            .resolve_name(&func.name.value, true, &ast.get_func_span(&func_id))
            .unwrap_or(ERROR_DEF);

        // Push function rib
        self.push_rib(RibKind::Function {
            def_id: func_def_id,
            can_capture: false, // For now, no captures
        });

        // Add function name to its own scope (for recursion)
        self.ribs
            .last_mut()
            .unwrap()
            .bindings
            .insert(func.name.value.to_string(), func_def_id);

        // Add parameters
        for (idx, param) in func.parameters.iter().enumerate() {
            let param_span = ast.get_func_span(&func_id).clone(); // TODO: get actual param span
            let def_id = self.define(
                &param.name.value,
                DefKind::Parameter {
                    func: func_def_id,
                    index: idx,
                },
                param_span,
            );

            self.ribs
                .last_mut()
                .unwrap()
                .bindings
                .insert(param.name.value.to_string(), def_id);
        }

        // Resolve body
        self.resolve_expr(func.body, ast, Some(func_id));

        self.pop_rib();
    }

    /// Try to resolve a path's last segment as a variable
    fn resolve_path_as_var(&mut self, path: &Path, expr_id: Id<Expr>, ast: &Ast) {
        let name = path.name();
        let span = ast.get_expr_span(&expr_id).clone();
        match self.resolve_name(&name.value, false, &span) {
            Ok(def_id) => {
                self.var_resolutions.insert(expr_id, def_id);
            }
            Err(err) => {
                self.errors.push(err);
                self.var_resolutions.insert(expr_id, ERROR_DEF);
            }
        }
    }

    /// Resolve an expression
    fn resolve_expr(&mut self, expr_id: Id<Expr>, ast: &Ast, current_func: Option<Id<Func>>) {
        match ast.exprs.get(&expr_id) {
            Expr::Hole | Expr::I32(_) | Expr::F32(_) | Expr::Bool(_) | Expr::String(_) => {
                // Nothing to resolve
            }

            Expr::Var(path) => {
                self.resolve_path_as_var(path, expr_id, ast);
            }

            Expr::Call { name: _, args } => {
                // name is a Path - function resolution happens at type-check time
                for arg in args {
                    self.resolve_expr(*arg, ast, current_func);
                }
            }

            Expr::Binary { lhs, rhs, .. } => {
                self.resolve_expr(*lhs, ast, current_func);
                self.resolve_expr(*rhs, ast, current_func);
            }

            Expr::Block { stmts, value } => {
                self.push_rib(RibKind::Block);

                for stmt in stmts {
                    self.resolve_stmt(*stmt, ast, current_func);
                }

                if let Some(val) = value {
                    self.resolve_expr(*val, ast, current_func);
                }

                self.pop_rib();
            }

            Expr::Borrow { expr, .. } => {
                self.resolve_expr(*expr, ast, current_func);
            }

            Expr::Deref { expr } => {
                self.resolve_expr(*expr, ast, current_func);
            }

            Expr::Not { expr } => {
                self.resolve_expr(*expr, ast, current_func);
            }

            Expr::Conditional { condition, truthy, falsy } => {
                self.resolve_expr(*condition, ast, current_func);
                self.resolve_expr(*truthy, ast, current_func);
                self.resolve_expr(*falsy, ast, current_func);
            }

            Expr::Constructor { struct_name: _, fields } => {
                for (_, field_expr) in fields {
                    self.resolve_expr(*field_expr, ast, current_func);
                }
            }

            Expr::FieldAccess { object, .. } => {
                self.resolve_expr(*object, ast, current_func);
            }

            Expr::Assignment { target, value } => {
                self.resolve_expr(*target, ast, current_func);
                self.resolve_expr(*value, ast, current_func);
            }
        }
    }

    /// Resolve a statement
    fn resolve_stmt(&mut self, stmt_id: Id<Stmt>, ast: &Ast, current_func: Option<Id<Func>>) {
        match ast.stmts.get(&stmt_id) {
            Stmt::Let { name, value, .. } => {
                // Resolve value FIRST (before adding binding)
                self.resolve_expr(*value, ast, current_func);

                // Then add binding to current scope
                let stmt_span = ast.get_stmt_span(&stmt_id).clone();
                let def_id = self.define(&name.value, DefKind::Variable, stmt_span);

                self.ribs
                    .last_mut()
                    .unwrap()
                    .bindings
                    .insert(name.value.to_string(), def_id);
            }

            Stmt::Expr { expr } => {
                self.resolve_expr(*expr, ast, current_func);
            }

            Stmt::Loop { body } => {
                self.push_rib(RibKind::Block);
                for stmt in body {
                    self.resolve_stmt(*stmt, ast, current_func);
                }
                self.pop_rib();
            }

            Stmt::While { condition, body } => {
                self.resolve_expr(*condition, ast, current_func);
                self.push_rib(RibKind::Block);
                for stmt in body {
                    self.resolve_stmt(*stmt, ast, current_func);
                }
                self.pop_rib();
            }

            Stmt::Condition { condition, then_body, else_body } => {
                self.resolve_expr(*condition, ast, current_func);

                self.push_rib(RibKind::Block);
                for stmt in then_body {
                    self.resolve_stmt(*stmt, ast, current_func);
                }
                self.pop_rib();

                if let Some(else_stmts) = else_body {
                    self.push_rib(RibKind::Block);
                    for stmt in else_stmts {
                        self.resolve_stmt(*stmt, ast, current_func);
                    }
                    self.pop_rib();
                }
            }
        }
    }

    /// Resolve a name to a DefId
    fn resolve_name(&self, name: &str, is_function_call: bool, span: &Span) -> Result<DefId, ResolveError> {
        // Search ribs from innermost to outermost
        for rib in self.ribs.iter().rev() {
            if let Some(&def_id) = rib.bindings.get(name) {
                return Ok(def_id);
            }

            // Function rib blocks access to outer locals (but not outer functions)
            if let RibKind::Function { can_capture: false, .. } = rib.kind {
                if !is_function_call {
                    continue;
                }
            }
        }

        // Not found
        if is_function_call {
            Err(ResolveError::UnresolvedFunction {
                name: name.to_string(),
                span: span.clone(),
            })
        } else {
            Err(ResolveError::UnresolvedVariable {
                name: name.to_string(),
                span: span.clone(),
            })
        }
    }
}

use crate::{
    Ast, Decl, Expr, ExternBlock, ExternFunc, Func, Ident, Module, Span, Stmt, Type,
    arena::Id,
};

/// Builder that handles all AST allocation and span tracking.
/// Decouples parsing logic from AST construction.
pub struct AstBuilder {
    pub ast: Ast,
    /// Stack of span start positions for automatic tracking
    span_stack: Vec<Span>,
    /// Counter for generating unique identifier IDs
    next_ident_id: u32,
}

impl AstBuilder {
    pub fn new() -> Self {
        Self {
            ast: Ast::new(),
            span_stack: Vec::new(),
            next_ident_id: 0,
        }
    }

    // --- Span tracking ---

    /// Start tracking a span - call at the beginning of a construct
    pub fn start_span(&mut self, span: Span) {
        self.span_stack.push(span);
    }

    /// Get the current span start without popping
    pub fn peek_span(&self) -> Option<&Span> {
        self.span_stack.last()
    }

    /// Pop and return the span start
    pub fn pop_span(&mut self) -> Span {
        self.span_stack.pop().expect("mismatched start_span/pop_span")
    }

    // --- Identifier creation ---

    pub fn make_ident(&mut self, value: &str) -> Ident {
        let id = self.next_ident_id;
        self.next_ident_id += 1;
        Ident {
            id,
            value: value.into(),
        }
    }

    // --- Expression allocation ---

    /// Allocate an expression with an explicit span
    pub fn alloc_expr(&mut self, expr: Expr, span: Span) -> Id<Expr> {
        self.ast.alloc_expr_with_span(expr, span)
    }

    /// Complete a span from the stack and allocate an expression
    pub fn finish_expr(&mut self, expr: Expr, end_span: Span) -> Id<Expr> {
        let start = self.pop_span();
        let span = start.merge(&end_span);
        self.ast.alloc_expr_with_span(expr, span)
    }

    /// Allocate a hole expression (for error recovery)
    pub fn alloc_hole(&mut self, span: Span) -> Id<Expr> {
        self.ast.alloc_expr_with_span(Expr::Hole, span)
    }

    // --- Statement allocation ---

    /// Allocate a statement with an explicit span
    pub fn alloc_stmt(&mut self, stmt: Stmt, span: Span) -> Id<Stmt> {
        self.ast.alloc_stmt_with_span(stmt, span)
    }

    /// Complete a span from the stack and allocate a statement
    pub fn finish_stmt(&mut self, stmt: Stmt, end_span: Span) -> Id<Stmt> {
        let start = self.pop_span();
        let span = start.merge(&end_span);
        self.ast.alloc_stmt_with_span(stmt, span)
    }

    // --- Function allocation ---

    /// Allocate a function with an explicit span
    pub fn alloc_func(&mut self, func: Func, span: Span) -> Id<Func> {
        self.ast.alloc_func_with_span(func, span)
    }

    /// Complete a span from the stack and allocate a function
    pub fn finish_func(&mut self, func: Func, end_span: Span) -> Id<Func> {
        let start = self.pop_span();
        let span = start.merge(&end_span);
        self.ast.alloc_func_with_span(func, span)
    }

    // --- Extern function allocation ---

    /// Allocate an extern function with an explicit span
    pub fn alloc_extern_func(&mut self, func: ExternFunc, span: Span) -> Id<ExternFunc> {
        self.ast.alloc_extern_func_with_span(func, span)
    }

    // --- Type span tracking ---

    /// Allocate a type span (for error reporting on type annotations)
    pub fn alloc_type_span(&mut self, span: Span) -> Id<Type> {
        self.ast.alloc_type_with_span(span)
    }

    // --- Module building ---

    /// Add a declaration to the current module
    pub fn add_decl(&mut self, decl: Decl) {
        // Ensure we have at least one module
        if self.ast.mods.is_empty() {
            self.ast.mods.push(Module {
                name: "main".into(),
                decs: Vec::new(),
            });
        }
        self.ast.mods.last_mut().unwrap().decs.push(decl);
    }

    /// Add a function declaration
    pub fn add_func(&mut self, func_id: Id<Func>) {
        self.add_decl(Decl::Func(func_id));
    }

    /// Add an extern block declaration
    pub fn add_extern_block(&mut self, block: ExternBlock) {
        self.add_decl(Decl::ExternBlock(block));
    }

    // --- Finalization ---

    /// Consume the builder and return the completed AST
    pub fn into_ast(self) -> Ast {
        self.ast
    }

    // --- Span access (for postfix operators that need the LHS span) ---

    /// Get the span of an expression
    pub fn get_expr_span(&self, id: &Id<Expr>) -> Span {
        self.ast.get_expr_span(id).clone()
    }
}

impl Default for AstBuilder {
    fn default() -> Self {
        Self::new()
    }
}

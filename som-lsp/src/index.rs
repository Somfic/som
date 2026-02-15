use som::arena::Id;
use som::{Ast, Decl, Expr, ExternFunc, Func, Module, Span, Stmt, Struct};
use std::collections::HashMap;

/// Reference to an AST node
#[derive(Debug, Clone)]
pub enum NodeRef {
    Expr(Id<Expr>),
    Stmt(Id<Stmt>),
    Func(Id<Func>),
    ExternFunc(Id<ExternFunc>),
    Struct(Id<Struct>),
}

/// Entry in the AST index: a byte range mapped to a node
#[derive(Debug, Clone)]
struct IndexEntry {
    start_offset: usize,
    end_offset: usize,
    node: NodeRef,
}

/// Reverse index from source positions to AST nodes, grouped by file
pub struct AstIndex {
    entries_by_file: HashMap<String, Vec<IndexEntry>>,
}

impl AstIndex {
    /// Build an index from an AST
    pub fn build(ast: &Ast) -> Self {
        let mut entries_by_file: HashMap<String, Vec<IndexEntry>> = HashMap::new();

        // Index all expressions
        for (expr_id, _) in ast.exprs.iter_with_ids() {
            let span = ast.get_expr_span(&expr_id);
            let file = span.source.identifier().to_string();
            entries_by_file.entry(file).or_default().push(IndexEntry {
                start_offset: span.start_offset,
                end_offset: span.start_offset + span.length,
                node: NodeRef::Expr(expr_id),
            });
        }

        // Index all statements
        for (stmt_id, _) in ast.stmts.iter_with_ids() {
            let span = ast.get_stmt_span(&stmt_id);
            let file = span.source.identifier().to_string();
            entries_by_file.entry(file).or_default().push(IndexEntry {
                start_offset: span.start_offset,
                end_offset: span.start_offset + span.length,
                node: NodeRef::Stmt(stmt_id),
            });
        }

        // Index all functions
        for (func_id, _) in ast.funcs.iter_with_ids() {
            let span = ast.get_func_span(&func_id);
            let file = span.source.identifier().to_string();
            entries_by_file.entry(file).or_default().push(IndexEntry {
                start_offset: span.start_offset,
                end_offset: span.start_offset + span.length,
                node: NodeRef::Func(func_id),
            });
        }

        // Index all extern functions
        for (efunc_id, _) in ast.extern_funcs.iter_with_ids() {
            let span = ast.get_extern_func_span(&efunc_id);
            let file = span.source.identifier().to_string();
            entries_by_file.entry(file).or_default().push(IndexEntry {
                start_offset: span.start_offset,
                end_offset: span.start_offset + span.length,
                node: NodeRef::ExternFunc(efunc_id),
            });
        }

        // Index all structs
        for (struct_id, _) in ast.structs.iter_with_ids() {
            let span = ast.get_struct_span(&struct_id);
            let file = span.source.identifier().to_string();
            entries_by_file.entry(file).or_default().push(IndexEntry {
                start_offset: span.start_offset,
                end_offset: span.start_offset + span.length,
                node: NodeRef::Struct(struct_id),
            });
        }

        // Sort each file's entries by span size (smallest first) for innermost-first lookup
        for entries in entries_by_file.values_mut() {
            entries.sort_by_key(|e| e.end_offset - e.start_offset);
        }

        AstIndex { entries_by_file }
    }

    /// Find the innermost AST node at a byte offset in a file
    pub fn find_at(&self, file: &str, offset: usize) -> Option<&NodeRef> {
        let entries = self.entries_by_file.get(file)?;

        // Find the smallest (innermost) span containing the offset
        entries
            .iter()
            .find(|e| offset >= e.start_offset && offset < e.end_offset)
            .map(|e| &e.node)
    }

    /// Find the file path for a module
    pub fn find_module_for_file<'a>(ast: &'a Ast, file_path: &str) -> Option<&'a Module> {
        ast.mods.iter().find(|m| {
            let module_path = m.path.to_string_lossy();
            module_path == file_path || file_path.ends_with(&*module_path)
        })
    }

    /// Get all declarations in a module, with their spans
    pub fn get_module_symbols(ast: &Ast, module: &Module) -> Vec<(String, NodeRef, Span)> {
        let mut symbols = Vec::new();

        for decl in &module.decs {
            match decl {
                Decl::Func(func_id) => {
                    let func = ast.funcs.get(func_id);
                    let span = ast.get_func_span(func_id).clone();
                    symbols.push((func.name.value.to_string(), NodeRef::Func(*func_id), span));
                }
                Decl::Struct(struct_id) => {
                    let s = ast.structs.get(struct_id);
                    let span = ast.get_struct_span(struct_id).clone();
                    symbols.push((s.name.value.to_string(), NodeRef::Struct(*struct_id), span));
                }
                Decl::ExternBlock(block) => {
                    for efunc_id in &block.functions {
                        let efunc = ast.extern_funcs.get(efunc_id);
                        let span = ast.get_extern_func_span(efunc_id).clone();
                        symbols.push((
                            efunc.name.value.to_string(),
                            NodeRef::ExternFunc(*efunc_id),
                            span,
                        ));
                    }
                }
                _ => {}
            }
        }

        symbols
    }
}

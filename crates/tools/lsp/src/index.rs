use som_ast::{Ast, Expr, Func, Id, Item, Span, Stmt, Struct, TypeExpr};
use std::collections::HashMap;

/// An AST node found at a source position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRef {
    Expr(Id<Expr>),
    Stmt(Id<Stmt>),
    Type(Id<TypeExpr>),
    Func(Id<Func>),
    Struct(Id<Struct>),
}

struct IndexEntry {
    start: usize,
    end: usize,
    node: NodeRef,
}

/// Maps source positions back to AST nodes, per file.
pub struct AstIndex {
    entries_by_file: HashMap<String, Vec<IndexEntry>>,
}

impl AstIndex {
    pub fn build(ast: &Ast) -> Self {
        let mut entries_by_file: HashMap<String, Vec<IndexEntry>> = HashMap::new();
        let mut add = |span: &Span, node: NodeRef| {
            entries_by_file
                .entry(span.source.identifier().to_string())
                .or_default()
                .push(IndexEntry {
                    start: span.start_offset,
                    end: span.start_offset + span.length,
                    node,
                });
        };

        for (id, _) in ast.exprs.iter() {
            add(ast.exprs.span(id), NodeRef::Expr(id));
        }
        for (id, _) in ast.stmts.iter() {
            add(ast.stmts.span(id), NodeRef::Stmt(id));
        }
        for (id, _) in ast.types.iter() {
            add(ast.types.span(id), NodeRef::Type(id));
        }
        for (id, _) in ast.funcs.iter() {
            add(ast.funcs.span(id), NodeRef::Func(id));
        }
        for (id, _) in ast.structs.iter() {
            add(ast.structs.span(id), NodeRef::Struct(id));
        }

        // Smallest spans first, so lookups find the innermost node.
        for entries in entries_by_file.values_mut() {
            entries.sort_by_key(|e| e.end - e.start);
        }
        AstIndex { entries_by_file }
    }

    /// The innermost node at a byte offset in a file.
    pub fn find_at(&self, file: &str, offset: usize) -> Option<NodeRef> {
        self.entries_by_file
            .get(file)?
            .iter()
            .find(|e| offset >= e.start && offset < e.end)
            .map(|e| e.node)
    }
}

/// The items declared in `file`, from whichever module it belongs to.
pub fn items_in_file<'a>(ast: &'a Ast, file: &str) -> Vec<&'a Item> {
    let in_file = |span: &Span| span.source.identifier() == file;
    ast.modules
        .iter()
        .flat_map(|m| &m.items)
        .filter(|item| match item {
            Item::Func(id) => in_file(ast.funcs.span(*id)),
            Item::Struct(id) => in_file(ast.structs.span(*id)),
            Item::Extern(block) => block
                .funcs
                .first()
                .is_some_and(|&f| in_file(ast.funcs.span(f))),
            Item::Impl(block) => in_file(&block.target.span()),
            Item::Trait(id) => in_file(ast.traits.span(*id)),
            Item::Use(u) => in_file(&u.path.span()),
        })
        .collect()
}

use som_ast::{Expr, Id, Stmt};
use som_driver::Analysis;
use som_resolve::DefKind;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Range};

use super::Cursor;
use crate::convert;
use crate::docs::{
    array_method_doc, array_of, describe, doc_of, field_doc, func_doc, signature, signature_with,
    struct_doc, struct_text, ty,
};
use crate::index::{AstIndex, NodeRef};
use crate::lookup::{extern_at, jump_span, loop_header, loop_of};
use crate::types::method_subst;

/// How many of the declarations an `extern` skipped its hover lists.
const SKIPPED_SHOWN: usize = 8;

/// Some text to show as som, and documentation to show below it.
type Shown = (Option<String>, Option<String>);

pub(crate) fn hover(a: &Analysis, cursor: &Cursor) -> Option<Hover> {
    let value = hover_markdown(a, &cursor.file, cursor.offset)?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: hover_range(a, cursor),
    })
}

/// `break x` and a range are each one thing: the hover covers all of it, not just the word.
fn hover_range(a: &Analysis, cursor: &Cursor) -> Option<Range> {
    let found = AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset);
    match found.map(|node| hover_node(a, node)) {
        Some(NodeRef::Stmt(id)) => jump_span(a, id).map(|span| convert::span_to_range(&span)),
        Some(NodeRef::Expr(id)) if matches!(a.ast.exprs[id], Expr::Range { .. }) => {
            Some(convert::span_to_range(a.ast.exprs.span(id)))
        }
        _ => None,
    }
}

/// The thing under the cursor as som, and its documentation below it.
pub(crate) fn hover_markdown(a: &Analysis, file: &str, offset: usize) -> Option<String> {
    if let Some(block) = extern_at(a, file, offset) {
        return Some(extern_hover(a, block));
    }
    let node = hover_node(a, AstIndex::build(&a.ast).find_at(file, offset)?);
    let (text, doc) = match node {
        NodeRef::Expr(id) => expr_hover(a, id),
        NodeRef::Type(id) => match a.names.ty(id) {
            Some(def) => (Some(describe(a, def)), doc_of(a, def)),
            None => (None, None),
        },
        NodeRef::Func(id) => (Some(signature(a, id)), func_doc(a, id)),
        NodeRef::Struct(id) => (Some(struct_text(a, id)), struct_doc(a, id)),
        NodeRef::Stmt(id) => (stmt_hover(a, id), None),
    };
    let text = text.filter(|t| !t.contains("{unknown}"))?;
    Some(match doc {
        Some(doc) => format!("```som\n{text}\n```\n\n{doc}"),
        None => format!("```som\n{text}\n```"),
    })
}

/// What hovering over `node` describes: a literal bound of a range is part of one thing, so
/// `1` in `1..4` stands for the whole range. A bound that's a name or a calculation, like `n`
/// in `0..n`, stays itself: its type is worth seeing.
fn hover_node(a: &Analysis, node: NodeRef) -> NodeRef {
    let NodeRef::Expr(id) = node else {
        return node;
    };
    if !matches!(a.ast.exprs[id], Expr::Int(_) | Expr::Float(_)) {
        return node;
    }
    a.ast
        .exprs
        .iter()
        .find(|(_, e)| matches!(e, Expr::Range { start, end } if *start == Some(id) || *end == Some(id)))
        .map_or(node, |(range, _)| NodeRef::Expr(range))
}

fn expr_hover(a: &Analysis, id: Id<Expr>) -> Shown {
    // Inside `if x { … }`, an optional `x` is the value it holds.
    if a.typed.is_narrowed(id) {
        let name = match &a.ast.exprs[id] {
            Expr::Var(path) => format!("{path}"),
            Expr::Field { field, .. } => format!(".{}", field.name),
            _ => String::new(),
        };
        let ty = ty(a, a.typed.expr(id));
        let text = if name.is_empty() {
            ty
        } else {
            format!("{name}: {ty}")
        };
        return (Some(text), None);
    }
    match &a.ast.exprs[id] {
        Expr::Var(_) | Expr::Out(_) | Expr::Call { .. } | Expr::Construct { .. } => {
            match a.typed.callee(id, &a.names) {
                Some(def) => (Some(describe(a, def)), doc_of(a, def)),
                None => (None, None),
            }
        }
        Expr::MethodCall { receiver, .. } => method_call_hover(a, id, *receiver),
        Expr::Field { object, .. }
            if let Some(method) = a.typed.array_method(id)
                && let Some(sequence) = array_of(a, a.typed.expr(*object)) =>
        {
            let doc = array_method_doc(a, method, sequence);
            (Some(doc.signature), Some(doc.doc))
        }
        Expr::Field { object, field } => (
            Some(format!(".{}: {}", field.name, ty(a, a.typed.expr(id)))),
            field_doc(a, *object, a.typed.field(id)),
        ),
        _ => (Some(ty(a, a.typed.expr(id))), None),
    }
}

fn method_call_hover(a: &Analysis, id: Id<Expr>, receiver: Id<Expr>) -> Shown {
    match a.typed.method(id) {
        Some(def) => match a.names.def(def).kind {
            DefKind::Func(f) => {
                let subst = method_subst(a, f, a.typed.expr(receiver));
                (Some(signature_with(a, f, &subst)), doc_of(a, def))
            }
            _ => (Some(describe(a, def)), doc_of(a, def)),
        },
        None => match (
            a.typed.array_method(id),
            array_of(a, a.typed.expr(receiver)),
        ) {
            (Some(method), Some(sequence)) => {
                let doc = array_method_doc(a, method, sequence);
                (Some(doc.signature), Some(doc.doc))
            }
            _ => (None, None),
        },
    }
}

fn stmt_hover(a: &Analysis, id: Id<Stmt>) -> Option<String> {
    match &a.ast.stmts[id] {
        Stmt::Let { .. } => a.names.local(id).map(|def| describe(a, def)),
        // `break x` names the loop like a label: the loop it leaves or goes on with.
        Stmt::Break(_) | Stmt::Continue(_) => loop_of(a, id).map(|l| {
            let header = loop_header(a, l).get_text();
            header.split_whitespace().collect::<Vec<_>>().join(" ")
        }),
        _ => None,
    }
}

/// What an `extern "raylib";` brings in: where the library and header are, how much was
/// imported, and what was skipped and why.
fn extern_hover(a: &Analysis, block: &som_ast::Extern) -> String {
    let mut text = extern_declaration(block);
    let header = block.header.as_ref().map(|(h, _)| h.as_str());
    let root = &a.ast.modules[0].path;
    let (library_file, header_file) =
        som_driver::locate(block.library.as_deref(), header, root, &Default::default());
    let found = |file: Option<std::path::PathBuf>| match file {
        Some(file) => format!("`{}`", file.display()),
        None => "not found".to_string(),
    };
    if block.library.is_some() {
        text.push_str(&format!("\n**library** {}  ", found(library_file)));
    }
    text.push_str(&format!("\n**header** {}", found(header_file)));
    if let Some(module) = block.module.map(|m| &a.ast.modules[m]) {
        text.push_str(&imported_summary(module));
    }
    text
}

/// The `extern` as written: `extern "raylib";` implies its header, at the library's name.
fn extern_declaration(block: &som_ast::Extern) -> String {
    let written = block
        .header
        .as_ref()
        .filter(|(_, span)| Some(span) != block.library_span.as_ref())
        .map(|(h, _)| h.as_str());
    match (&block.library, written) {
        (Some(library), Some(header)) => {
            format!("```som\nextern {library:?} from {header:?}\n```\n")
        }
        (Some(library), None) => format!("```som\nextern {library:?}\n```\n"),
        (None, Some(header)) => format!("```som\nextern from {header:?}\n```\n"),
        (None, None) => "```som\nextern\n```\n".to_string(),
    }
}

/// What's in the som generated from a header, and what it says it skipped.
fn imported_summary(module: &som_ast::Module) -> String {
    let Some(generated) = module.files.first() else {
        return String::new();
    };
    let mut funcs = 0;
    let mut structs = 0;
    for item in &module.items {
        match item {
            som_ast::Item::Struct(_) => structs += 1,
            som_ast::Item::Extern(b) => funcs += b.funcs.len(),
            som_ast::Item::Func(_) => funcs += 1,
            _ => {}
        }
    }
    let skipped: Vec<&str> = generated
        .content()
        .lines()
        .filter_map(|line| line.strip_prefix("// skipped "))
        .collect();
    let mut text = format!(
        "\n\n**imported** {funcs} functions and {structs} structs into `{}`",
        module.name
    );
    if !skipped.is_empty() {
        text.push_str(&format!("\n\n**skipped** {}:\n", skipped.len()));
        for line in skipped.iter().take(SKIPPED_SHOWN) {
            let (name, reason) = line.split_once(": ").unwrap_or((line, ""));
            text.push_str(&format!("\n- `{name}`: {reason}"));
        }
        if skipped.len() > SKIPPED_SHOWN {
            text.push_str(&format!(
                "\n- and {} more, listed in `{}`",
                skipped.len() - SKIPPED_SHOWN,
                generated.identifier()
            ));
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_parameters_show_as_out() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn divide(a: i32, b: i32, out remainder: i32) -> i32 {\n    \
                      remainder = a % b;\n    a / b\n}\n\n\
                      fn main() { let q = divide(17, 5, out remainder); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let file = main.to_str().unwrap();
        let hover = |needle: &str| hover_markdown(&a, file, source.find(needle).unwrap() + 1);

        let signature = "fn divide(a: i32, b: i32, out remainder: i32) -> i32";
        assert_eq!(
            hover("divide(17").as_deref(),
            Some(format!("```som\n{signature}\n```").as_str())
        );
        // Inside the function, and the variable `out` declares at the call.
        assert_eq!(
            hover("remainder = ").as_deref(),
            Some("```som\nout remainder: i32\n```")
        );
        assert_eq!(
            hover_markdown(&a, file, source.find("out remainder)").unwrap() + 5).as_deref(),
            Some("```som\nremainder: i32\n```")
        );
    }

    #[test]
    fn hover_shows_documentation() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "/// A point.\nstruct Point {\n    /// Across.\n    x: i32,\n}\n\n\
                      /// Squares a number.\n/// Twice.\nfn square(n: i32) -> i32 { n * n }\n\n\
                      fn main() {\n    let p = Point { x: 2 };\n    let y = square(p.x);\n}\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let file = main.to_str().unwrap();
        let hover = |needle: &str| hover_markdown(&a, file, source.find(needle).unwrap() + 1);

        assert_eq!(
            hover("square(p").as_deref(),
            Some("```som\nfn square(n: i32) -> i32\n```\n\nSquares a number.\nTwice.")
        );
        assert!(hover("Point {").unwrap().ends_with("```\n\nA point."));
        assert!(
            hover("p.x")
                .map(|_| ())
                .and(hover_markdown(&a, file, source.find("p.x").unwrap() + 2))
                .unwrap()
                .ends_with("```\n\nAcross.")
        );
        assert_eq!(
            hover("main").as_deref(),
            Some("```som\nfn main()\n```"),
            "no documentation, no gap"
        );
    }
}

//! What's at, around or in scope at a place in a program.

use std::collections::HashSet;
use std::path::PathBuf;

use som_ast::{Expr, Id, Item, Span, Stmt, Struct, TypeExpr};
use som_driver::Analysis;
use som_resolve::visibility::Viewer;
use som_resolve::{DefId, DefKind};

use crate::docs::readable_fields;
use crate::types::struct_of;

/// Whether `span` is in `file` and `offset` is in it or at either end.
pub(crate) fn covers(span: &Span, file: &str, offset: usize) -> bool {
    span.source.identifier() == file
        && (span.start_offset..=span.start_offset + span.length).contains(&offset)
}

fn end(span: &Span) -> usize {
    span.start_offset + span.length
}

/// Whether `inner` is within `outer`, both in the same file.
fn within(inner: &Span, outer: &Span) -> bool {
    outer.source.identifier() == inner.source.identifier()
        && outer.start_offset <= inner.start_offset
        && end(inner) <= end(outer)
}

/// The variables visible at `offset` in `file`: the parameters of the function it's in, and
/// the `let`, `for` and `out` variables declared before it in a block around it. A later
/// declaration of the same name hides an earlier one.
pub(crate) fn locals_at(a: &Analysis, file: &str, offset: usize) -> Vec<DefId> {
    let contains = |span: &Span| covers(span, file, offset);
    let Some((func, func_span)) = (a.ast.funcs.iter())
        .map(|(id, _)| (id, a.ast.funcs.span(id)))
        .filter(|(_, span)| contains(span))
        .min_by_key(|(_, span)| span.length)
    else {
        return Vec::new();
    };
    let scopes: Vec<&Span> = (a.ast.exprs.iter())
        .filter(|(_, e)| matches!(e, Expr::Block { .. }))
        .map(|(id, _)| a.ast.exprs.span(id))
        .chain(
            (a.ast.stmts.iter())
                .filter(|(_, s)| {
                    matches!(
                        s,
                        Stmt::If { .. } | Stmt::While { .. } | Stmt::Loop { .. } | Stmt::For { .. }
                    )
                })
                .map(|(id, _)| a.ast.stmts.span(id)),
        )
        .collect();
    // While a line is still being typed, the function's body can end before the cursor: what's
    // declared in it is visible anywhere in the function.
    let body = a.ast.funcs[func].body.map(|b| a.ast.exprs.span(b));
    let scope_of = |declared: &Span| {
        let innermost = scopes
            .iter()
            .filter(|scope| {
                scope.source.identifier() == file
                    && scope.start_offset <= declared.start_offset
                    && end(declared) <= end(scope)
                    && scope.length > declared.length
            })
            .min_by_key(|scope| scope.length)
            .copied()?;
        let is_body = body.is_some_and(|b| {
            b.start_offset == innermost.start_offset && b.length == innermost.length
        });
        Some(if is_body { func_span } else { innermost })
    };

    let mut found: Vec<(usize, DefId)> = a
        .names
        .params(func)
        .iter()
        .map(|&def| (a.names.def(def).span.start_offset, def))
        .collect();
    for (def, d) in a.names.defs.iter_with_ids() {
        let scope = match d.kind {
            DefKind::Local(stmt) if matches!(a.ast.stmts[stmt], Stmt::For { .. }) => {
                Some(a.ast.stmts.span(stmt))
            }
            DefKind::Local(stmt) => scope_of(a.ast.stmts.span(stmt)),
            DefKind::Out(expr) => scope_of(a.ast.exprs.span(expr)),
            _ => continue,
        };
        let declared_before = d.span.source.identifier() == file && d.span.start_offset < offset;
        let in_function =
            func_span.start_offset <= d.span.start_offset && d.span.start_offset <= end(func_span);
        if declared_before && in_function && scope.is_some_and(contains) {
            found.push((d.span.start_offset, def));
        }
    }
    found.sort_by_key(|&(at, _)| std::cmp::Reverse(at));
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|&(_, def)| seen.insert(a.names.def(def).name.clone()))
        .map(|(_, def)| def)
        .collect()
}

/// Who's looking from `offset` in `file`: its module, the file, and the type whose impl
/// block it's in, if any.
pub(crate) fn viewer_at<'f>(a: &Analysis, file: &'f str, offset: usize) -> Viewer<'f> {
    let module = (a.ast.modules.iter())
        .position(|m| m.files.iter().any(|f| f.identifier() == file))
        .unwrap_or(usize::MAX);
    let owner = (a.ast.funcs.iter())
        .filter(|&(f, _)| covers(a.ast.funcs.span(f), file, offset))
        .find_map(|(f, _)| a.names.impl_of(f))
        .and_then(|i| a.names.impls[i].owner);
    Viewer {
        module,
        file,
        owner,
    }
}

/// The module `file` is in, and the modules whose names it can use bare: its own, the ones it
/// `use`s, and the standard library's root.
pub(crate) fn modules_in_scope(a: &Analysis, file: &str, offset: usize) -> (usize, Vec<usize>) {
    let module_named = |name: &str| a.ast.modules.iter().position(|m| *m.name == *name);
    let here = a
        .ast
        .modules
        .iter()
        .position(|m| m.files.iter().any(|f| f.identifier() == file))
        .unwrap_or(0);
    let mut in_scope = vec![here];
    for item in &a.ast.modules[here].items {
        if let Item::Use(u) = item
            && let Some(m) = module_named(&u.path.to_string())
        {
            in_scope.push(m);
        }
    }
    // A `use` inside a block is in scope for the rest of it.
    for (id, stmt) in a.ast.stmts.iter() {
        let Stmt::Use(u) = stmt else { continue };
        let at = a.ast.stmts.span(id);
        if at.source.identifier() != file || end(at) > offset {
            continue;
        }
        let block = (a.ast.exprs.iter())
            .filter(|(_, e)| matches!(e, Expr::Block { .. }))
            .map(|(e, _)| a.ast.exprs.span(e))
            .filter(|b| b.source.identifier() == file && within(at, b))
            .min_by_key(|b| b.length);
        let inside = block.is_some_and(|b| offset < end(b));
        if inside
            && let Some(m) = module_named(&u.path.to_string())
            && !in_scope.contains(&m)
        {
            in_scope.push(m);
        }
    }
    if let Some(prelude) = module_named(som_lang::PRELUDE_MODULE)
        && !in_scope.contains(&prelude)
    {
        in_scope.push(prelude);
    }
    // Each with what it passes on through `pub use`, and what those pass on.
    let mut at = 0;
    while at < in_scope.len() {
        for item in &a.ast.modules[in_scope[at]].items {
            if let Item::Use(u) = item
                && u.public
                && let Some(m) = module_named(&u.path.to_string())
                && !in_scope.contains(&m)
            {
                in_scope.push(m);
            }
        }
        at += 1;
    }
    (here, in_scope)
}

/// The module `path` names from `offset` in `file`: by its whole name, `som::io`, or by how
/// it ends, for one in scope there: `io` after `use som::io;`.
pub(crate) fn module_named(a: &Analysis, path: &str, file: &str, offset: usize) -> Option<usize> {
    (a.ast.modules.iter().position(|m| *m.name == *path)).or_else(|| {
        let tail = format!("::{path}");
        (modules_in_scope(a, file, offset).1.into_iter())
            .find(|&m| a.ast.modules[m].name.ends_with(&*tail))
    })
}

/// Whether module `m` is a library's that module `here` has no `extern` for, so what's in it
/// can't be named there.
pub(crate) fn undeclared_library(a: &Analysis, here: usize, m: usize) -> bool {
    let externs = |module: usize| {
        a.ast.modules[module]
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Extern(block) if block.library.is_some() => block.module,
                _ => None,
            })
    };
    let is_library = (0..a.ast.modules.len()).any(|module| externs(module).any(|l| l == m));
    is_library && !externs(here).any(|l| l == m)
}

/// The struct `path` names: `raylib::Color` in that module, `Color` in the modules in scope,
/// or through an alias of it.
pub(crate) fn find_struct(
    a: &Analysis,
    path: &str,
    file: &str,
    offset: usize,
) -> Option<Id<Struct>> {
    let modules = match path.rsplit_once("::") {
        Some((module, _)) => module_named(a, module, file, offset).into_iter().collect(),
        None => modules_in_scope(a, file, offset).1,
    };
    let name = path.rsplit("::").next().unwrap_or(path);
    modules.into_iter().find_map(|m| {
        a.ast.modules[m].items.iter().find_map(|item| match item {
            Item::Struct(id) if *a.ast.structs[*id].name.name == *name => Some(*id),
            Item::Alias(alias) if *alias.name.name == *name => aliased_struct(a, alias.ty),
            _ => None,
        })
    })
}

/// The struct type `ty`, an alias's, names, through any aliases it's written with.
pub(crate) fn aliased_struct(a: &Analysis, mut ty: Id<TypeExpr>) -> Option<Id<Struct>> {
    // Bounded, because an alias can refer to itself.
    for _ in 0..=a.names.aliases().len() {
        match a.names.def(a.names.ty(ty)?).kind {
            DefKind::Struct(s) => return Some(s),
            DefKind::Alias { ty: next } => ty = next,
            _ => return None,
        }
    }
    None
}

pub(crate) fn field_definition(
    a: &Analysis,
    object: Id<Expr>,
    index: Option<usize>,
) -> Option<Span> {
    let owner = struct_of(a, a.typed.expr(object))?;
    let DefKind::Struct(s) = a.names.def(owner).kind else {
        return None;
    };
    Some(readable_fields(a, s).get(index?)?.0.name.span.clone())
}

/// The expression a dot at `dot` is written after: the outermost one ending there. A
/// parenthesized one, `(a + b).`, ends before its `)`. Or for `.x`, short for `self.x`, the
/// `self` at the dot.
pub(crate) fn receiver_before(
    a: &Analysis,
    file: &str,
    text: &str,
    dot: usize,
) -> Option<Id<Expr>> {
    let ending_at = |end_at: usize, parenthesized: bool| {
        a.ast
            .exprs
            .iter()
            .map(|(id, _)| (id, a.ast.exprs.span(id)))
            .filter(|&(id, span)| {
                span.source.identifier() == file
                    && end(span) == end_at
                    && (parenthesized || can_have_dot(a, id))
            })
            .max_by_key(|(_, span)| span.length)
            .map(|(id, _)| id)
    };
    let mut before = text[..dot].trim_end();
    let mut parenthesized = false;
    let receiver = loop {
        if let Some(expr) = ending_at(before.len(), parenthesized) {
            break Some(expr);
        }
        match before.strip_suffix(')') {
            Some(inner) => (before, parenthesized) = (inner.trim_end(), true),
            None => break None,
        }
    };
    receiver.or_else(|| {
        (a.ast.exprs.iter())
            .find(|&(id, expr)| {
                let span = a.ast.exprs.span(id);
                matches!(expr, Expr::Var(path) if &*path.to_string() == som_lang::SELF_VALUE)
                    && span.source.identifier() == file
                    && span.start_offset == dot
                    && span.length == 1
            })
            .map(|(id, _)| id)
    })
}

/// Whether `expr` can be what's before a `.` without parentheses: not `a + b`, where the dot
/// goes with `b`, and not a call missing its `)`, as in `free(alloc(10).`, where it goes with
/// the argument.
fn can_have_dot(a: &Analysis, expr: Id<Expr>) -> bool {
    let end_of = |id: Id<Expr>| end(a.ast.exprs.span(id));
    match &a.ast.exprs[expr] {
        Expr::Binary { .. }
        | Expr::Not(_)
        | Expr::Neg(_)
        | Expr::BitNot(_)
        | Expr::Borrow { .. }
        | Expr::Deref(_)
        | Expr::Assign { .. }
        | Expr::Conditional { .. } => false,
        Expr::Call { args, .. } | Expr::MethodCall { args, .. } => {
            args.last().is_none_or(|&last| end_of(last) != end_of(expr))
        }
        _ => true,
    }
}

/// Where `break`, `continue` or `break x` is written, without the `;`: the one thing hover
/// and go to definition point at.
pub(crate) fn jump_span(a: &Analysis, stmt: Id<Stmt>) -> Option<Span> {
    let (Stmt::Break(named) | Stmt::Continue(named)) = &a.ast.stmts[stmt] else {
        return None;
    };
    let whole = a.ast.stmts.span(stmt);
    let stop = match named {
        Some(name) => end(&name.span),
        None => {
            let keyword = match a.ast.stmts[stmt] {
                Stmt::Break(_) => "break",
                _ => "continue",
            };
            whole.start_offset + keyword.len()
        }
    };
    Some(Span::from_range(
        whole.start_offset..stop,
        whole.source.clone(),
    ))
}

/// The loop `stmt` leaves or goes on with, if it's `break` or `continue`: the `for` loop it
/// names, or else the innermost loop around it.
pub(crate) fn loop_of(a: &Analysis, stmt: Id<Stmt>) -> Option<Id<Stmt>> {
    let (Stmt::Break(named) | Stmt::Continue(named)) = &a.ast.stmts[stmt] else {
        return None;
    };
    if named.is_some() {
        return match a.names.def(a.names.loop_target(stmt)?).kind {
            DefKind::Local(for_stmt) => Some(for_stmt),
            _ => None,
        };
    }
    let at = a.ast.stmts.span(stmt);
    a.ast
        .stmts
        .iter()
        .filter(|(_, s)| matches!(s, Stmt::Loop { .. } | Stmt::While { .. } | Stmt::For { .. }))
        .filter(|(id, _)| within(at, a.ast.stmts.span(*id)))
        .min_by_key(|(id, _)| a.ast.stmts.span(*id).length)
        .map(|(id, _)| id)
}

/// A loop's header, without its body: `for x in 1..4`, `while i < n`, `loop`.
pub(crate) fn loop_header(a: &Analysis, loop_stmt: Id<Stmt>) -> Span {
    let whole = a.ast.stmts.span(loop_stmt);
    let stop = match &a.ast.stmts[loop_stmt] {
        Stmt::For { iterable: last, .. }
        | Stmt::While {
            condition: last, ..
        } => end(a.ast.exprs.span(*last)),
        _ => whole.start_offset + "loop".len(),
    };
    Span::from_range(whole.start_offset..stop, whole.source.clone())
}

/// The `extern` whose library or header name is at `offset` in `file`.
pub(crate) fn extern_at<'a>(
    a: &'a Analysis,
    file: &str,
    offset: usize,
) -> Option<&'a som_ast::Extern> {
    a.ast
        .modules
        .iter()
        .flat_map(|m| &m.items)
        .find_map(|item| {
            let Item::Extern(block) = item else {
                return None;
            };
            let header = block.header.as_ref().map(|(_, span)| span);
            let at =
                (block.library_span.iter().chain(header)).any(|span| covers(span, file, offset));
            at.then_some(block)
        })
}

/// The names in `extern "raylib";` and `from "raylib.h"` in `file`, each with the som
/// generated from the header that it imports.
pub(crate) fn extern_links(a: &Analysis, file: &str) -> Vec<(Span, PathBuf)> {
    let mut links = Vec::new();
    for item in a.ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let generated = block
            .module
            .and_then(|m| a.ast.modules[m].files.first())
            .map(|source| PathBuf::from(source.identifier()));
        let Some(generated) = generated else { continue };
        let header = block.header.as_ref().map(|(_, span)| span);
        for span in block.library_span.iter().chain(header) {
            // In `extern "raylib";` the header is implied, at the library's name.
            let seen = links
                .iter()
                .any(|(s, _): &(Span, _)| s.start_offset == span.start_offset);
            if span.source.identifier() == file && !seen {
                links.push((span.clone(), generated.clone()));
            }
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structs_are_found_by_their_whole_path() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        std::fs::write(&main, "use som::io;\nfn main() -> i32 { 0 }\n").unwrap();
        let a = som_driver::analyze(dir.path(), &som_driver::Options::default());
        let file = main.to_string_lossy().to_string();
        let whole = find_struct(&a, "som::io::File", &file, 0);
        assert!(whole.is_some());
        assert_eq!(find_struct(&a, "io::File", &file, 0), whole);
        assert_eq!(find_struct(&a, "nope::File", &file, 0), None);
        // `som::io::File { ` can't be written here, its field being private: `open` makes one.
        let made: Vec<&str> =
            (crate::handlers::completion::items::constructors(&a, whole.unwrap(), &|_| true)
                .into_iter())
            .map(|(name, _, _)| name)
            .collect();
        assert_eq!(made, ["open"]);
    }

    #[test]
    fn a_for_loop_without_its_body_still_types_what_it_loops_over() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let text = "fn main() -> i32 {\n    let file = som::io::File::open(\"a\", som::io::FileMode::Read);\n    if file == none @unreachable;\n    for line in file\n    0\n}\n";
        std::fs::write(&main, text).unwrap();
        let a = som_driver::analyze(dir.path(), &som_driver::Options::default());
        let file = main.to_string_lossy().to_string();
        let dot = text.find("in file").unwrap() + "in file".len();
        let receiver = receiver_before(&a, &file, text, dot).unwrap();
        let owner = struct_of(&a, a.typed.expr(receiver)).unwrap();
        assert_eq!(&*a.names.def(owner).name, "File");
    }
}

use std::path::Path;

use som_ast::{Expr, Id, Span, Stmt};
use som_driver::Analysis;
use tower_lsp::lsp_types::{
    DocumentLink, GotoDefinitionResponse, Location, LocationLink, Range, Url,
    request::GotoDeclarationResponse,
};

use super::Cursor;
use super::rename::uses_at;
use crate::convert;
use crate::index::{AstIndex, NodeRef};
use crate::lookup::{extern_links, field_definition, jump_span, loop_header, loop_of};
use crate::origin;

pub(crate) fn definition(a: &Analysis, cursor: &Cursor) -> Option<GotoDefinitionResponse> {
    let (file, offset) = (&cursor.file, cursor.offset);
    if let Some((_, generated)) = extern_links(a, file)
        .into_iter()
        .find(|(span, _)| (span.start_offset..=span.start_offset + span.length).contains(&offset))
    {
        return Url::from_file_path(generated).ok().map(|uri| {
            GotoDefinitionResponse::Scalar(Location {
                uri,
                range: Range::default(),
            })
        });
    }
    // On a definition's own name, itself: the editor then shows its references instead.
    if let Some(uses) = uses_at(a, file, offset)
        && uses.at.definition
    {
        return convert::location(&uses.at.span).map(GotoDefinitionResponse::Scalar);
    }
    let target: Span = match AstIndex::build(&a.ast).find_at(file, offset)? {
        NodeRef::Expr(id) => expr_definition(a, id)?,
        NodeRef::Type(id) => a.names.def(a.names.ty(id)?).span.clone(),
        NodeRef::Stmt(id) => return loop_link(a, id),
        // In a file generated from a header, a declaration's definition is its C one.
        NodeRef::Func(id) => {
            return origin::of_func(a, id)
                .and_then(origin::location)
                .map(GotoDefinitionResponse::Scalar);
        }
        NodeRef::Struct(id) => {
            let location = origin::of_name(&a.ast.structs[id].name).and_then(origin::location);
            return location.map(GotoDefinitionResponse::Scalar);
        }
    };
    convert::location(&target).map(GotoDefinitionResponse::Scalar)
}

fn expr_definition(a: &Analysis, id: Id<Expr>) -> Option<Span> {
    let def = match &a.ast.exprs[id] {
        Expr::Var(_) | Expr::Call { .. } | Expr::Construct { .. } => {
            a.typed.callee(id, &a.names)?
        }
        Expr::MethodCall { .. } => a.typed.method(id)?,
        Expr::Field { object, .. } => match a.typed.method(id) {
            // A property, `list.len`.
            Some(d) => d,
            None => return field_definition(a, *object, a.typed.field(id)),
        },
        _ => return None,
    };
    Some(a.names.def(def).span.clone())
}

/// From all of `break x` to the loop it leaves, so the editor underlines all of it.
fn loop_link(a: &Analysis, id: Id<Stmt>) -> Option<GotoDefinitionResponse> {
    let (from, to) = (jump_span(a, id)?, loop_of(a, id)?);
    let to = loop_header(a, to);
    let target_uri = Url::from_file_path(to.source.identifier()).ok()?;
    let target = convert::span_to_range(&to);
    Some(GotoDefinitionResponse::Link(vec![LocationLink {
        origin_selection_range: Some(convert::span_to_range(&from)),
        target_uri,
        target_range: target,
        target_selection_range: target,
    }]))
}

/// For a function or struct imported from C: its declaration in the header.
pub(crate) fn declaration(a: &Analysis, cursor: &Cursor) -> Option<GotoDeclarationResponse> {
    let origin = match AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset)? {
        NodeRef::Func(id) => origin::of_func(a, id),
        NodeRef::Struct(id) => origin::of_name(&a.ast.structs[id].name),
        NodeRef::Expr(id) => {
            let def = match &a.ast.exprs[id] {
                Expr::MethodCall { .. } | Expr::Field { .. } => a.typed.method(id),
                _ => a.names.expr(id),
            };
            def.and_then(|d| origin::of_def(a, d))
        }
        NodeRef::Type(id) => a.names.ty(id).and_then(|d| origin::of_def(a, d)),
        NodeRef::Stmt(_) => None,
    };
    origin
        .and_then(origin::location)
        .map(GotoDeclarationResponse::Scalar)
}

/// The library and header names in `extern`s, to the som generated from them; and in a file
/// generated from a header, each function's and struct's name, to its C declaration.
pub(crate) fn document_links(a: &Analysis, file: &str) -> Vec<DocumentLink> {
    let mut links: Vec<DocumentLink> = extern_links(a, file)
        .into_iter()
        .filter_map(|(span, generated)| {
            Some(DocumentLink {
                range: convert::span_to_range(&span),
                target: Some(Url::from_file_path(&generated).ok()?),
                tooltip: Some(format!("Open {}", relative(&generated, file))),
                data: None,
            })
        })
        .collect();

    let origins = som_driver::origins(Path::new(file));
    let names = (a.ast.funcs.iter().map(|(_, f)| &f.name))
        .chain(a.ast.structs.iter().map(|(_, s)| &s.name));
    for ident in names {
        if ident.span.source.identifier() != file {
            continue;
        }
        let Some((header, line)) = origins.get(&*ident.name) else {
            continue;
        };
        let name = header.file_name().unwrap_or_default().to_string_lossy();
        links.push(DocumentLink {
            range: convert::span_to_range(&ident.span),
            target: origin::header_url(header, *line),
            tooltip: Some(format!("Open {name}:{line}")),
            data: None,
        });
    }
    links
}

/// `path` from the directory of `file`: `.som/raylib.som`.
fn relative(path: &Path, file: &str) -> String {
    Path::new(file)
        .parent()
        .and_then(|dir| path.strip_prefix(dir).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

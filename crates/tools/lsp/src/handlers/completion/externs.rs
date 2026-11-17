use std::collections::HashSet;

use som_ast::Item;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionItemLabelDetails};

use super::Request;
use crate::lookup::covers;
use crate::text_scan::extern_library;

/// The C standard library, which is always there and has no file of its own to find.
const LIBC: &str = "c";

/// After `extern ` or in `extern "ray`: the libraries som can find, but not ones this file
/// already has an `extern` for, other than the one being edited.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let quoted = extern_library(request.text, request.offset)?;
    let a = request.a;
    let written: HashSet<&str> = (a.ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Extern(block) => Some((block.library.as_deref()?, block.library_span.as_ref()?)),
            _ => None,
        })
        .filter(|(_, span)| {
            span.source.identifier() == request.file && !covers(span, request.file, request.offset)
        })
        .map(|(library, _)| library)
        .collect();
    let libc = (!written.contains(LIBC)).then(|| (LIBC.to_string(), None, Some("0".into())));
    let found = (request.importable.libraries.iter())
        .filter(|(name, _)| !written.contains(name.as_str()))
        .map(|(name, path)| (name.clone(), Some(path.display().to_string()), None));
    let items = libc
        .into_iter()
        .chain(found)
        .map(|(name, path, sort_text)| CompletionItem {
            insert_text: (!quoted).then(|| format!("{name:?}")),
            label: name,
            kind: Some(CompletionItemKind::MODULE),
            label_details: Some(CompletionItemLabelDetails {
                detail: None,
                description: Some(path.unwrap_or_else(|| "the C standard library".into())),
            }),
            sort_text,
            ..Default::default()
        })
        .collect();
    Some(items)
}

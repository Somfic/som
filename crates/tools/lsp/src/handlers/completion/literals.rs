use som_ast::{Id, Struct};
use som_resolve::visibility;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, Range, TextEdit,
};

use super::Request;
use super::items::{call_snippet, constructors};
use crate::convert::offset_to_position;
use crate::docs::{doc_of, markdown, signature, ty};
use crate::lookup::find_struct;
use crate::text_scan::{literal_end, struct_literal};

/// In a struct literal, `Color { r: 1, `: the fields not written yet. If some of its fields
/// can't be seen from here, it can't be made with a literal: what makes one, `File::open(…)`,
/// replaces it.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let (path, written) = struct_literal(request.text, request.offset)?;
    let a = request.a;
    let s = find_struct(a, path, request.file, request.offset)?;
    let hidden = |i| visibility::hidden_field(&a.ast, &a.names, s, i, request.viewer).is_some();
    let mut items = Vec::new();
    let fields = a.ast.structs[s].fields.iter().zip(a.typed.fields(s));
    for (i, (field, &t)) in fields.enumerate() {
        if written.contains(&*field.name.name) || hidden(i) {
            continue;
        }
        items.push(CompletionItem {
            label: field.name.name.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(ty(a, t)),
            documentation: markdown(field.doc.clone()),
            insert_text: Some(format!("{}: ", field.name.name)),
            ..Default::default()
        });
    }
    if (0..a.ast.structs[s].fields.len()).any(hidden) {
        items.extend(constructor_items(request, path, s));
    }
    Some(items)
}

/// The functions that make struct `s`, each replacing the literal from `path` to its end.
fn constructor_items(request: &Request, path: &str, s: Id<Struct>) -> Vec<CompletionItem> {
    let a = request.a;
    let text = request.text;
    let start = path.as_ptr() as usize - text.as_ptr() as usize;
    let range = Range::new(
        offset_to_position(text, start),
        offset_to_position(text, literal_end(text, request.offset)),
    );
    (constructors(a, s, &|def| request.visible(def)).into_iter())
        .map(|(name, f, method)| {
            let label = format!("{path}::{name}");
            let snippet = call_snippet(a, f, &label, 0, request.insert);
            CompletionItem {
                label: format!("{label}(…)"),
                kind: Some(CompletionItemKind::CONSTRUCTOR),
                detail: Some(signature(a, f)),
                documentation: markdown(doc_of(a, method)),
                filter_text: Some(name.to_string()),
                insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: snippet.unwrap_or(label),
                })),
                ..Default::default()
            }
        })
        .collect()
}

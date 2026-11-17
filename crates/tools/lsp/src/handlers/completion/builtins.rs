use som_ast::Item;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, CompletionTextEdit, TextEdit};

use super::Request;
use super::items::func_item;
use crate::docs::markdown;
use crate::lookup::modules_in_scope;

/// After `@`, as in `@unr`: the built-ins, then the functions in scope that can be called
/// while compiling. Each replaces what's typed of it, `@` and all.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let before = &request.text[..request.offset];
    let word = before
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
        .len();
    if !before[..word].ends_with('@') {
        return None;
    }
    let range = request.typed_range(request.offset - (word - 1));
    let mut items = Vec::new();
    for (name, doc) in (som_lang::Builtin::ALL.iter()).filter_map(|b| Some((b.name(), b.doc()?))) {
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            filter_text: Some(name.to_string()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: name.to_string(),
            })),
            documentation: markdown(Some(doc.to_string())),
            ..Default::default()
        });
    }
    // Not C's functions, which only exist when the program runs.
    let a = request.a;
    let (_, in_scope) = modules_in_scope(a, request.file, request.offset);
    for m in in_scope {
        for item in &a.ast.modules[m].items {
            let Item::Func(f) = *item else { continue };
            let func = &a.ast.funcs[f];
            if func.body.is_none() || !request.visible(a.names.func(f)) {
                continue;
            }
            let mut item = func_item(a, f, format!("@{}", func.name.name), request.insert);
            let text = item
                .insert_text
                .take()
                .unwrap_or_else(|| item.label.clone());
            item.filter_text = Some(item.label.clone());
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range,
                new_text: text,
            }));
            items.push(item);
        }
    }
    Some(items)
}

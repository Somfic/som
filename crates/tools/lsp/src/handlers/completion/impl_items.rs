use som_ast::Item;
use som_driver::Analysis;
use som_typeck::stubs::Stub;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, InsertTextFormat, TextEdit,
};

use super::Request;
use crate::text_scan::word_start;

/// In `impl Eq for Hello { … }`, where an item starts: what the block still has to write,
/// replacing the `fn e` typed so far.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let (stubs, start) = missing_items(request.a, request.file, request.text, request.offset)?;
    let range = request.typed_range(request.offset - start);
    let items = stubs
        .into_iter()
        .map(|stub| {
            let keyword = if stub.is_type { "type" } else { "fn" };
            CompletionItem {
                label: stub.signature.clone(),
                kind: Some(match stub.is_type {
                    true => CompletionItemKind::TYPE_PARAMETER,
                    false => CompletionItemKind::METHOD,
                }),
                detail: Some("not written yet".into()),
                filter_text: Some(format!("{keyword} {}", stub.name)),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: stub.snippet,
                })),
                preselect: Some(true),
                ..Default::default()
            }
        })
        .collect();
    Some(items)
}

/// Inside a trait's impl block, where an item starts, with maybe `fn` or `type` and part of a
/// name typed: what the block still has to write, and where what's typed of it starts.
fn missing_items(
    a: &Analysis,
    file: &str,
    text: &str,
    offset: usize,
) -> Option<(Vec<Stub>, usize)> {
    let word_start = word_start(text, offset);
    let line_start = text[..word_start].rfind('\n').map_or(0, |i| i + 1);
    let indent =
        text[line_start..word_start].len() - text[line_start..word_start].trim_start().len();
    let start = match text[line_start..word_start].trim() {
        "" => word_start,
        "fn" | "type" => line_start + indent,
        _ => return None,
    };
    let blocks = (a.ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Impl(block) => Some(block),
            _ => None,
        });
    for (index, block) in blocks.enumerate() {
        let header = block.target.span();
        if block.trait_.is_none()
            || block.close.source.identifier() != file
            || !(header.start_offset + header.length < offset && offset <= block.close.start_offset)
        {
            continue;
        }
        let in_method = block.methods.iter().any(|&m| {
            let span = a.ast.funcs.span(m);
            span.start_offset < start && offset <= span.start_offset + span.length
        });
        if in_method {
            return None;
        }
        let stubs = som_typeck::stubs::missing(&a.ast, &a.names, index, block);
        return (!stubs.is_empty()).then_some((stubs, start));
    }
    None
}

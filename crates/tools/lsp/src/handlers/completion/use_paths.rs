use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use super::Request;
use crate::lookup::modules_in_scope;
use crate::text_scan::use_path;

/// In `use som::`: the next part of the paths of the modules still to `use`.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let prefix = use_path(request.text, request.offset)?;
    let a = request.a;
    let (here, in_scope) = modules_in_scope(a, request.file, request.offset);
    let used: HashSet<String> = (in_scope.iter())
        .map(|&m| a.ast.modules[m].name.to_string())
        .collect();
    let names: Vec<String> = (a.ast.modules.iter().enumerate())
        .filter(|&(m, module)| m != here && !module.name.is_empty())
        .map(|(_, module)| module.name.to_string())
        .chain(request.importable.modules.iter().cloned())
        .filter(|name| !used.contains(name))
        .collect();
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for name in &names {
        let Some(rest) = name.strip_prefix(prefix) else {
            continue;
        };
        let next = rest.split("::").next().unwrap_or_default();
        if next.is_empty() || !seen.insert(next.to_string()) {
            continue;
        }
        items.push(CompletionItem {
            label: next.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(format!("{prefix}{next}")),
            ..Default::default()
        });
    }
    Some(items)
}

use tower_lsp::lsp_types::{Command, CompletionItem, CompletionItemKind, Range, TextEdit};

use super::Request;
use super::items::module_items;
use crate::docs::describe;
use crate::lookup::{locals_at, modules_in_scope, undeclared_library};

/// The editor's command that saves the file, so a module's import is loaded.
const SAVE_COMMAND: &str = "workbench.action.files.save";

/// Where nothing more specific goes: the variables in scope, the modules and what's in them,
/// the modules that could be imported, and the keywords.
pub(super) fn complete(request: &Request) -> Vec<CompletionItem> {
    let a = request.a;
    let mut items: Vec<CompletionItem> = locals_at(a, request.file, request.offset)
        .into_iter()
        .map(|def| CompletionItem {
            label: a.names.def(def).name.to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some(describe(a, def)),
            ..Default::default()
        })
        .collect();
    items.extend(module_completions(request));
    items.extend(import_completions(request));
    items.extend(som_lexer::KEYWORDS.iter().map(|keyword| CompletionItem {
        label: keyword.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        ..Default::default()
    }));
    items
}

/// This file's module, and the modules it `use`s, by bare name; other modules by theirs, so
/// `rayl` finds `raylib::init_window`. A library's only where its `extern` is written.
fn module_completions(request: &Request) -> Vec<CompletionItem> {
    let a = request.a;
    let (here, in_scope) = modules_in_scope(a, request.file, request.offset);
    let mut items = Vec::new();
    for (m, module) in a.ast.modules.iter().enumerate() {
        if undeclared_library(a, here, m) {
            continue;
        }
        if in_scope.contains(&m) {
            items.extend(module_items(a, m, None, request.insert));
        }
        if m != here && !module.name.is_empty() {
            items.push(CompletionItem {
                label: module.name.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                ..Default::default()
            });
            if !in_scope.contains(&m) {
                items.extend(module_items(a, m, Some(&module.name), request.insert));
            }
        }
    }
    items
}

/// Modules that aren't imported yet: choosing one adds the import. Libraries are offered in
/// `extern "` instead.
fn import_completions(request: &Request) -> Vec<CompletionItem> {
    let a = request.a;
    let loaded = |name: &str| a.ast.modules.iter().any(|m| *m.name == *name);
    (request.importable.modules.iter())
        .filter(|name| !loaded(name))
        .map(|name| {
            let line = format!("use {name};");
            CompletionItem {
                label: name.clone(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some(format!("add {line}")),
                insert_text: Some(format!("{name}::")),
                additional_text_edits: Some(vec![TextEdit {
                    range: Range::default(),
                    new_text: format!("{line}\n"),
                }]),
                // Its functions can be listed once the import is saved.
                command: Some(Command {
                    title: "save".into(),
                    command: SAVE_COMMAND.into(),
                    arguments: None,
                }),
                ..Default::default()
            }
        })
        .collect()
}

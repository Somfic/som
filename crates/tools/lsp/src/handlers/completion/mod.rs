//! Completion, by what the cursor is in: each provider answers if the cursor is where it
//! completes, in the order they're tried.

mod builtins;
mod externs;
mod impl_items;
pub(crate) mod items;
mod literals;
mod members;
mod qualified;
mod scope;
mod type_slots;
mod use_paths;

use som_driver::{Analysis, Importable};
use som_resolve::DefId;
use som_resolve::visibility::{self, Viewer};
use tower_lsp::lsp_types::{CompletionItem, Position, Range};

use crate::lookup::viewer_at;
use items::Insert;

/// Where completion was asked for, and what for.
pub(crate) struct Request<'a> {
    a: &'a Analysis,
    importable: &'a Importable,
    file: &'a str,
    text: &'a str,
    offset: usize,
    position: Position,
    insert: Insert,
    /// Only what can be used from here is offered: no private fields or methods of another
    /// file's type.
    viewer: Viewer<'a>,
}

impl<'a> Request<'a> {
    pub(crate) fn new(
        a: &'a Analysis,
        importable: &'a Importable,
        file: &'a str,
        text: &'a str,
        offset: usize,
        position: Position,
    ) -> Self {
        Request {
            a,
            importable,
            file,
            text,
            offset,
            position,
            insert: Insert::at(text, offset),
            viewer: viewer_at(a, file, offset),
        }
    }

    fn visible(&self, def: DefId) -> bool {
        visibility::can_see(&self.a.ast, &self.a.names, def, self.viewer)
    }

    /// From `typed` characters before the cursor to it: what a completion replaces.
    fn typed_range(&self, typed: usize) -> Range {
        let at = self.position;
        Range::new(
            Position::new(at.line, at.character.saturating_sub(typed as u32)),
            at,
        )
    }
}

/// The completions at the cursor; `trigger` is the character that asked for them, if one did.
pub(crate) fn complete(request: &Request, trigger: Option<&str>) -> Option<Vec<CompletionItem>> {
    if let Some(items) = builtins::complete(request) {
        return Some(items);
    }
    if let Some(items) = externs::complete(request) {
        return Some(items);
    }
    // A space or quote only asks for the libraries after `extern`.
    if matches!(trigger, Some(" " | "\"")) {
        return None;
    }
    let providers: [fn(&Request) -> Option<Vec<CompletionItem>>; 5] = [
        use_paths::complete,
        members::complete,
        impl_items::complete,
        literals::complete,
        type_slots::complete,
    ];
    if let Some(items) = providers.iter().find_map(|provider| provider(request)) {
        return Some(items);
    }
    match qualified::complete(request) {
        qualified::Answer::Items(items) => Some(items),
        qualified::Answer::Nothing => None,
        qualified::Answer::NotQualified => Some(scope::complete(request)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::analyzed;

    /// The labels completed at the end of `source`.
    fn labels_at_end(source: &str) -> Vec<String> {
        let (_dir, file, a) = analyzed(source);
        let importable = Importable { modules: Vec::new(), libraries: Vec::new() };
        let last = source.lines().count() as u32 - 1;
        let column = source.lines().last().unwrap().len() as u32;
        let request = Request::new(
            &a,
            &importable,
            &file,
            source,
            source.len(),
            Position::new(last, column),
        );
        (complete(&request, None).unwrap_or_default().into_iter())
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn the_standard_library_s_root_functions_complete_without_an_import() {
        let labels = labels_at_end("fn main() -> i32 {\n    ass");
        assert!(labels.iter().any(|l| l == "assert"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "assert_eq"), "{labels:?}");
    }
}

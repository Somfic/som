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
    use tower_lsp::lsp_types::CompletionItemKind;

    /// The items completed at the end of `source`.
    fn items_at_end(source: &str) -> Vec<CompletionItem> {
        let (_dir, file, a) = analyzed(source);
        let importable = Importable {
            modules: Vec::new(),
            libraries: Vec::new(),
        };
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
        complete(&request, None).unwrap_or_default()
    }

    #[test]
    fn a_types_functions_say_what_they_are() {
        let source = "struct Bag { n: i32 }\n\
                      impl Bag {\n\
                          pub fn empty() -> Self = Bag { n: 0 };\n\
                          pub fn twice(n: i32) -> i32 = n * 2;\n\
                          pub fn add(&mut self, n: i32) { .n = .n + n; }\n\
                          pub fn count(&self) -> i32 = .n;\n\
                      }\n\
                      fn main() -> i32 {\n    let b = Bag::";
        let mut items = items_at_end(source);
        items.sort_by(|x, y| x.sort_text.cmp(&y.sort_text));
        let shown: Vec<(String, Option<CompletionItemKind>, String, String)> = (items.into_iter())
            .map(|item| {
                let details = item.label_details.unwrap_or_default();
                let (detail, description) = (details.detail, details.description);
                let (detail, description) =
                    (detail.unwrap_or_default(), description.unwrap_or_default());
                (item.label, item.kind, detail, description)
            })
            .collect();
        let (constructor, function, method) = (
            Some(CompletionItemKind::CONSTRUCTOR),
            Some(CompletionItemKind::FUNCTION),
            Some(CompletionItemKind::METHOD),
        );
        let s = |x: &str| x.to_string();
        assert_eq!(
            shown,
            vec![
                (s("empty"), constructor, s("()"), s("makes one")),
                (s("twice"), function, s("(n)"), s("function")),
                (s("add"), method, s("(&mut self, n)"), s("method")),
                (s("count"), method, s("(&self)"), s("method")),
            ]
        );
    }

    #[test]
    fn a_value_has_only_what_takes_self() {
        let source = "struct Bag { n: i32 }\n\
                      impl Bag {\n\
                          pub fn empty() -> Self = Bag { n: 0 };\n\
                          pub fn count(&self) -> i32 = .n;\n\
                      }\n\
                      fn main() -> i32 {\n    let b = Bag::empty();\n    b.count()\n}\n";
        let (_dir, file, a) = analyzed(source);
        let importable = Importable {
            modules: Vec::new(),
            libraries: Vec::new(),
        };
        // Just after `b.`, in the program as it was analyzed.
        let offset = source.find("b.count").unwrap() + 2;
        let line = source[..offset].lines().count() as u32 - 1;
        let column = source[..offset].lines().last().unwrap().len() as u32;
        let request = Request::new(
            &a,
            &importable,
            &file,
            source,
            offset,
            Position::new(line, column),
        );
        let labels: Vec<String> = (complete(&request, None).unwrap_or_default().into_iter())
            .map(|i| i.label)
            .collect();
        assert!(labels.iter().any(|l| l == "count"), "{labels:?}");
        assert!(!labels.iter().any(|l| l == "empty"), "{labels:?}");
    }

    #[test]
    fn tests_are_not_offered() {
        let source = "@test\nfn adds_up() {}\n\nfn adds() {}\n\nfn main() -> i32 {\n    add";
        let labels: Vec<String> = items_at_end(source).into_iter().map(|i| i.label).collect();
        assert!(labels.iter().any(|l| l == "adds"), "{labels:?}");
        assert!(!labels.iter().any(|l| l.contains("adds_up")), "{labels:?}");
    }

    /// The labels completed at the end of `source`.
    fn labels_at_end(source: &str) -> Vec<String> {
        let (_dir, file, a) = analyzed(source);
        let importable = Importable {
            modules: Vec::new(),
            libraries: Vec::new(),
        };
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

    /// The labels completed just after `marker`'s first `.` in `source`, as analyzed.
    fn labels_after(source: &str, marker: &str) -> Vec<String> {
        let (_dir, file, a) = analyzed(source);
        let importable = Importable {
            modules: Vec::new(),
            libraries: Vec::new(),
        };
        let at = source.find(marker).unwrap();
        let offset = at + marker.find('.').unwrap() + 1;
        let line = source[..offset].lines().count() as u32 - 1;
        let column = source[..offset].lines().last().unwrap().len() as u32;
        let request = Request::new(
            &a,
            &importable,
            &file,
            source,
            offset,
            Position::new(line, column),
        );
        (complete(&request, None).unwrap_or_default().into_iter())
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn a_slice_completes_its_traits_methods() {
        let source = "fn main() -> i32 {\n    let a: &[i32] = [1, 2, 3, 4];\n    \
                      let n = a.len;\n    n as i32\n}\n";
        let labels = labels_after(source, "a.len");
        assert!(labels.iter().any(|l| l == "len"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "iter"), "{labels:?}");
    }

    #[test]
    fn a_struct_completes_what_a_blanket_impl_gives_it() {
        let source = "fn main() -> i32 {\n    let a: &[i32] = [1, 2, 3, 4];\n    \
                      let i = a.iter();\n    let n = i.next();\n    0\n}\n";
        let labels = labels_after(source, "i.next");
        assert!(labels.iter().any(|l| l == "next"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "where"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "collect"), "{labels:?}");
    }

    #[test]
    fn a_trait_s_methods_without_pub_are_not_completed_on_values() {
        let source = "fn main() -> i32 {\n    let list: List<i32> = List::empty();\n    \
                      let n = list.len;\n    let x = 3;\n    let y = x.max(4);\n    0\n}\n";
        let labels = labels_after(source, "list.len");
        assert!(labels.iter().any(|l| l == "push"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "len"), "{labels:?}");
        assert!(!labels.iter().any(|l| l == "add"), "{labels:?}");
        let labels = labels_after(source, "x.max");
        assert!(!labels.iter().any(|l| l == "compare"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "to_string"), "{labels:?}");
    }

    #[test]
    fn a_list_completes_the_methods_its_iterator_has() {
        let source = "fn main() -> i32 {\n    let list: List<i32> = List::empty();\n    \
                      let n = list.len;\n    let a: &[i32] = [1, 2];\n    let m = a.len;\n    0\n}\n";
        for marker in ["list.len", "a.len"] {
            let labels = labels_after(source, marker);
            assert!(labels.iter().any(|l| l == "where"), "{marker}: {labels:?}");
            assert!(labels.iter().any(|l| l == "collect"), "{marker}: {labels:?}");
            assert!(!labels.iter().any(|l| l == "next"), "{marker}: {labels:?}");
        }
    }

    #[test]
    fn a_reference_to_a_reference_completes_what_it_refers_to() {
        let source = "fn main() -> i32 {\n    let s: string = \"ab\";\n    let r = &s;\n    \
                      let rr = &r;\n    let n = rr.len;\n    0\n}\n";
        let labels = labels_after(source, "rr.len");
        assert!(labels.iter().any(|l| l == "len"), "{labels:?}");
    }

    #[test]
    fn the_standard_library_s_root_functions_complete_without_an_import() {
        let labels = labels_at_end("fn main() -> i32 {\n    ass");
        assert!(labels.iter().any(|l| l == "assert"), "{labels:?}");
        assert!(labels.iter().any(|l| l == "assert_eq"), "{labels:?}");
    }
}

use som_common::{GenArena, GenId};

/// A renderer node. Handles to it are generational `GenId<Node>` keys, so a key
/// that outlives its node (removed by the reconciler, then its slot reused)
/// fails a lookup instead of silently addressing a different node.
pub struct Node {
    kind: NodeKind,
}

enum NodeKind {
    Element { tag: String },
    Text,
}

/// Marker for event-handler ids. The *runtime* owns the handler table and mints
/// these; the renderer only stores one on `listen` and echoes it back when the
/// backend reports an event. Keeping it an opaque `GenId<Handler>` is the
/// reactivity firewall — the renderer never sees a closure.
pub struct Handler;

/// A dumb executor of tree mutations. Zero reactivity awareness: no closures, no
/// state queries — those are runtime concerns. Node handles are opaque.
///
/// Uses `&str` (not `impl Into<String>`) so the trait stays object-safe: the
/// runtime holds the renderer behind a `dyn Renderer` port.
pub trait Renderer {
    fn create_element(&mut self, tag: &str) -> GenId<Node>;
    fn create_text(&mut self) -> GenId<Node>;

    /// Positional insert. `before: None` appends; otherwise `child` lands
    /// immediately before `before`. The `before` slot is load-bearing for the
    /// keyed reconciler even though a hand-written counter only ever appends.
    fn insert(&mut self, parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>);
    fn remove(&mut self, node: GenId<Node>);

    fn set_text(&mut self, node: GenId<Node>, text: &str);
    fn set_attr(&mut self, node: GenId<Node>, name: &str, value: &str);
    fn remove_attr(&mut self, node: GenId<Node>, name: &str);
    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool);

    fn listen(&mut self, node: GenId<Node>, event: &str, handler: GenId<Handler>);
}

/// A `Renderer` that records every call as a log line and mints real
/// generational keys from a `GenArena`. The log is the assertion surface for
/// snapshot tests; the arena gives keys their fail-safe stale semantics and
/// makes `remove` real, so reused-slot bugs surface here rather than downstream.
#[derive(Default)]
pub struct MockRenderer {
    nodes: GenArena<Node>,
    log: Vec<String>,
}

impl MockRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded call sequence.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// A readable, stable label for a node in the log, e.g. `main#0`, `text#1`.
    /// A key whose node is gone renders as `#idx(gone)` rather than aliasing.
    fn node_ref(&self, id: GenId<Node>) -> String {
        match self.nodes.get(id) {
            Some(Node {
                kind: NodeKind::Element { tag },
            }) => format!("{tag}{id}"),
            Some(Node {
                kind: NodeKind::Text,
            }) => format!("text{id}"),
            None => format!("{id}(gone)"),
        }
    }
}

impl Renderer for MockRenderer {
    fn create_element(&mut self, tag: &str) -> GenId<Node> {
        let id = self.nodes.insert(Node {
            kind: NodeKind::Element {
                tag: tag.to_string(),
            },
        });
        self.log.push(format!("create_element({tag:?}) -> {id}"));
        id
    }

    fn create_text(&mut self) -> GenId<Node> {
        let id = self.nodes.insert(Node {
            kind: NodeKind::Text,
        });
        self.log.push(format!("create_text() -> {id}"));
        id
    }

    fn insert(&mut self, parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>) {
        let before = before.map_or_else(|| "None".to_string(), |b| self.node_ref(b));
        self.log.push(format!(
            "insert(parent={}, child={}, before={before})",
            self.node_ref(parent),
            self.node_ref(child),
        ));
    }

    fn remove(&mut self, node: GenId<Node>) {
        // Log before removing so the node still has a readable label.
        self.log.push(format!("remove({})", self.node_ref(node)));
        self.nodes.remove(node);
    }

    fn set_text(&mut self, node: GenId<Node>, text: &str) {
        self.log
            .push(format!("set_text({}, {text:?})", self.node_ref(node)));
    }

    fn set_attr(&mut self, node: GenId<Node>, name: &str, value: &str) {
        self.log.push(format!(
            "set_attr({}, {name:?}, {value:?})",
            self.node_ref(node)
        ));
    }

    fn remove_attr(&mut self, node: GenId<Node>, name: &str) {
        self.log
            .push(format!("remove_attr({}, {name:?})", self.node_ref(node)));
    }

    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool) {
        self.log.push(format!(
            "set_class({}, {class:?}, {on})",
            self.node_ref(node)
        ));
    }

    fn listen(&mut self, node: GenId<Node>, event: &str, handler: GenId<Handler>) {
        self.log.push(format!(
            "listen({}, {event:?}, handler={handler})",
            self.node_ref(node)
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_the_call_sequence() {
        let mut r = MockRenderer::new();

        // Mimics building `main > text` and a reactive text/class write.
        let root = r.create_element("main");
        let text = r.create_text();
        r.insert(root, text, None);
        r.set_text(text, "Count: 0");
        r.set_class(root, "active", true);

        // Handler ids are minted by whoever owns the handler table (here, a
        // throwaway arena standing in for the runtime).
        let mut handlers: GenArena<Handler> = GenArena::new();
        let click = handlers.insert(Handler);
        r.listen(root, "click", click);

        // A later reactive update writes the same text node again.
        r.set_text(text, "Count: 1");

        assert_eq!(
            r.log(),
            &[
                r#"create_element("main") -> #0"#,
                "create_text() -> #1",
                "insert(parent=main#0, child=text#1, before=None)",
                r#"set_text(text#1, "Count: 0")"#,
                r#"set_class(main#0, "active", true)"#,
                r#"listen(main#0, "click", handler=#0)"#,
                r#"set_text(text#1, "Count: 1")"#,
            ]
        );
    }

    #[test]
    fn removed_node_key_does_not_alias_reuse() {
        let mut r = MockRenderer::new();

        let a = r.create_element("div"); // #0
        r.remove(a);
        let b = r.create_element("span"); // reuses slot #0, new generation

        // Same index in the log, but the keys are distinct — the stale `a`
        // can never be mistaken for `b`.
        assert_ne!(a, b);
        assert_eq!(
            r.log(),
            &[
                r#"create_element("div") -> #0"#,
                "remove(div#0)",
                r#"create_element("span") -> #0"#,
            ]
        );
    }
}

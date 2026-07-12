use som_canvas::{Attribute, Event, Handler, Node, Renderer, Tag};
use som_common::{GenArena, GenId};

#[derive(Default)]
pub struct MockRenderer {
    nodes: GenArena<Node>,
    log: Vec<String>,
    handlers: Vec<(GenId<Node>, Event, GenId<Handler>)>,
}

impl MockRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn log(&self) -> &[String] {
        &self.log
    }

    pub fn handler_for(&self, node: GenId<Node>, event: Event) -> Option<GenId<Handler>> {
        self.handlers
            .iter()
            .rev()
            .find(|(n, e, _)| *n == node && *e == event)
            .map(|(_, _, h)| *h)
    }

    fn node_ref(&self, id: GenId<Node>) -> String {
        match self.nodes.get(id) {
            Some(node) => format!("{}{id}", node.tag),
            None => format!("{id}(gone)"),
        }
    }
}

impl Renderer for MockRenderer {
    fn create_element(&mut self, tag: Tag) -> GenId<Node> {
        let id = self.nodes.insert(Node { tag });
        self.log
            .push(format!("create_element({:?}) -> {id}", tag.to_string()));
        id
    }

    fn create_text(&mut self) -> GenId<Node> {
        let id = self.nodes.insert(Node { tag: Tag::Text });
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
        self.log.push(format!("remove({})", self.node_ref(node)));
        self.nodes.remove(node);
    }

    fn set_text(&mut self, node: GenId<Node>, text: &str) {
        self.log
            .push(format!("set_text({}, {text:?})", self.node_ref(node)));
    }

    fn set_attr(&mut self, node: GenId<Node>, name: Attribute, value: &str) {
        self.log.push(format!(
            "set_attr({}, {name:?}, {value:?})",
            self.node_ref(node)
        ));
    }

    fn remove_attr(&mut self, node: GenId<Node>, name: Attribute) {
        self.log
            .push(format!("remove_attr({}, {name:?})", self.node_ref(node)));
    }

    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool) {
        self.log.push(format!(
            "set_class({}, {class:?}, {on})",
            self.node_ref(node)
        ));
    }

    fn listen(&mut self, node: GenId<Node>, event: Event, handler: GenId<Handler>) {
        self.log.push(format!(
            "listen({}, {:?}, handler={handler})",
            self.node_ref(node),
            event.to_string()
        ));
        self.handlers.push((node, event, handler));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_the_call_sequence() {
        let mut r = MockRenderer::new();

        let root = r.create_element(Tag::Main);
        let text = r.create_text();
        r.insert(root, text, None);
        r.set_text(text, "Count: 0");
        r.set_class(root, "active", true);

        let mut handlers: GenArena<Handler> = GenArena::new();
        let click = handlers.insert(Handler);
        r.listen(root, Event::Click, click);

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

        let a = r.create_element(Tag::Block);
        r.remove(a);
        let b = r.create_element(Tag::Text);

        assert_ne!(a, b);
        assert_eq!(
            r.log(),
            &[
                r#"create_element("block") -> #0"#,
                "remove(block#0)",
                r#"create_element("text") -> #0"#,
            ]
        );
    }
}

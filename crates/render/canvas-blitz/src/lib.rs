use blitz_dom::{
    BaseDocument, DocumentConfig, DocumentMutator, LocalName, QualName, local_name, ns,
};
use som_canvas::{Event, Handler, Node, Renderer, Tag};
use som_common::{GenArena, GenId};
use std::collections::HashMap;

pub struct BlitzRenderer {
    doc: BaseDocument,
    nodes: GenArena<usize>,
    classes: HashMap<usize, Vec<String>>,
    handlers: Vec<(GenId<Node>, Event, GenId<Handler>)>,
    root: GenId<Node>,
}

impl BlitzRenderer {
    pub fn new() -> Self {
        let doc = BaseDocument::new(DocumentConfig::default());
        let mut nodes = GenArena::new();
        // Blitz's document root is node 0
        let root = nodes.insert(0usize).cast();
        Self {
            doc,
            nodes,
            classes: HashMap::new(),
            handlers: Vec::new(),
            root,
        }
    }

    pub fn root(&self) -> GenId<Node> {
        self.root
    }

    fn blitz_id(&self, node: GenId<Node>) -> usize {
        *self.nodes.get(node.cast()).expect("unknown blitz node")
    }
}

impl Default for BlitzRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer for BlitzRenderer {
    fn create_element(&mut self, tag: Tag) -> GenId<Node> {
        let qual = element_name(tag);
        let blitz_id = DocumentMutator::new(&mut self.doc).create_element(qual, Vec::new());
        self.nodes.insert(blitz_id).cast()
    }

    fn create_text(&mut self) -> GenId<Node> {
        let blitz_id = DocumentMutator::new(&mut self.doc).create_text_node("");
        self.nodes.insert(blitz_id).cast()
    }

    fn insert(&mut self, parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>) {
        let parent = self.blitz_id(parent);
        let child = self.blitz_id(child);
        let anchor = before.map(|b| self.blitz_id(b));
        let mut m = DocumentMutator::new(&mut self.doc);
        match anchor {
            None => m.append_children(parent, &[child]),
            Some(a) => m.insert_nodes_before(a, &[child]),
        }
    }

    fn remove(&mut self, node: GenId<Node>) {
        let blitz_id = self.blitz_id(node);
        DocumentMutator::new(&mut self.doc).remove_node(blitz_id);
        self.nodes.remove(node.cast());
        self.classes.remove(&blitz_id);
    }

    fn set_text(&mut self, node: GenId<Node>, text: &str) {
        let blitz_id = self.blitz_id(node);
        DocumentMutator::new(&mut self.doc).set_node_text(blitz_id, text);
    }

    fn set_attr(&mut self, node: GenId<Node>, name: &str, value: &str) {
        let blitz_id = self.blitz_id(node);
        let qual = attribute_name(name);
        DocumentMutator::new(&mut self.doc).set_attribute(blitz_id, qual, value);
    }

    fn remove_attr(&mut self, node: GenId<Node>, name: &str) {
        let blitz_id = self.blitz_id(node);
        let qual = attribute_name(name);
        DocumentMutator::new(&mut self.doc).clear_attribute(blitz_id, qual);
    }

    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool) {
        let blitz_id = self.blitz_id(node);
        let joined = {
            let list = self.classes.entry(blitz_id).or_default();
            let present = list.iter().any(|c| c == class);
            if on && !present {
                list.push(class.to_string());
            } else if !on && present {
                list.retain(|c| c != class);
            }
            list.join(" ")
        };
        let qual = QualName::new(None, ns!(), local_name!("class"));
        DocumentMutator::new(&mut self.doc).set_attribute(blitz_id, qual, &joined);
    }

    fn listen(&mut self, node: GenId<Node>, event: Event, handler: GenId<Handler>) {
        // Real event wiring (blitz-shell -> runtime::dispatch) lands in Increment 3.
        self.handlers.push((node, event, handler));
    }
}

fn element_name(tag: Tag) -> QualName {
    let local = match tag {
        Tag::Main => local_name!("main"),
        Tag::Block => local_name!("div"),
        Tag::Button => local_name!("button"),
        Tag::Text => panic!("text nodes are created via create_text, not create_element"),
    };
    QualName::new(None, ns!(html), local)
}

fn attribute_name(name: &str) -> QualName {
    QualName::new(None, ns!(), LocalName::from(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_blitz_dom_tree() {
        let mut r = BlitzRenderer::new();
        let root = r.root();

        let main = r.create_element(Tag::Main);
        r.insert(root, main, None);

        let text = r.create_text();
        r.insert(main, text, None);
        r.set_text(text, "Count: 0");

        let button = r.create_element(Tag::Button);
        r.insert(root, button, None);

        let root_bid = r.blitz_id(root);
        let main_bid = r.blitz_id(main);
        let text_bid = r.blitz_id(text);
        let button_bid = r.blitz_id(button);

        let m = DocumentMutator::new(&mut r.doc);

        assert!(m.child_ids(root_bid).contains(&main_bid));
        assert!(m.child_ids(root_bid).contains(&button_bid));
        assert_eq!(m.child_ids(main_bid), vec![text_bid]);
        assert_eq!(
            m.element_name(main_bid).map(|q| q.local.to_string()),
            Some("main".to_string())
        );
        assert_eq!(
            m.element_name(button_bid).map(|q| q.local.to_string()),
            Some("button".to_string())
        );
    }
}

use std::collections::HashMap;

use blitz_dom::{
    BaseDocument, DEFAULT_CSS, DocumentConfig, DocumentMutator, LocalName, QualName, local_name, ns,
};
use som_canvas::{Attribute, Event, Handler, Node, Renderer, Tag};
use som_common::{GenArena, GenId};

pub struct BlitzRenderer {
    pub(crate) doc: BaseDocument,
    pub(crate) nodes: GenArena<usize>,
    pub(crate) classes: HashMap<usize, Vec<String>>,
    pub(crate) handlers: Vec<(GenId<Node>, Event, GenId<Handler>)>,
    root: GenId<Node>,
}

impl BlitzRenderer {
    pub fn new() -> Self {
        let config = DocumentConfig {
            ua_stylesheets: Some(vec![String::from(DEFAULT_CSS)]),
            ..Default::default()
        };
        let mut doc = BaseDocument::new(config);
        let mut nodes = GenArena::new();

        let body = {
            let mut m = DocumentMutator::new(&mut doc);
            let html = m.create_element(
                QualName::new(None, ns!(html), local_name!("html")),
                Vec::new(),
            );
            let body = m.create_element(
                QualName::new(None, ns!(html), local_name!("body")),
                Vec::new(),
            );
            m.append_children(0, &[html]);
            m.append_children(html, &[body]);
            body
        };
        let root = nodes.insert(body).cast();

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

    pub(crate) fn blitz_id(&self, node: GenId<Node>) -> usize {
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

    fn set_attr(&mut self, node: GenId<Node>, name: Attribute, value: &str) {
        let blitz_id = self.blitz_id(node);
        let qual = attribute_name(name);
        DocumentMutator::new(&mut self.doc).set_attribute(blitz_id, qual, value);
    }

    fn remove_attr(&mut self, node: GenId<Node>, name: Attribute) {
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

fn attribute_name(attribute: Attribute) -> QualName {
    let local = match attribute {
        Attribute::Style => "style",
    };

    QualName::new(None, ns!(), LocalName::from(local))
}

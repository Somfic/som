use std::ops::Index;

use som_base::{Id, Span};

/// An arena of syntax nodes, each with the span it was parsed from.
pub struct Nodes<T> {
    nodes: Vec<T>,
    spans: Vec<Span>,
}

impl<T> Default for Nodes<T> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            spans: Vec::new(),
        }
    }
}

impl<T> Nodes<T> {
    pub fn alloc(&mut self, node: T, span: Span) -> Id<T> {
        let id = Id::new(self.nodes.len());
        self.nodes.push(node);
        self.spans.push(span);
        id
    }

    pub fn span(&self, id: Id<T>) -> &Span {
        &self.spans[id.id]
    }

    /// Replaces a node and its span, keeping its id.
    pub fn replace(&mut self, id: Id<T>, node: T, span: Span) {
        self.nodes[id.id] = node;
        self.spans[id.id] = span;
    }

    pub fn get_mut(&mut self, id: Id<T>) -> &mut T {
        &mut self.nodes[id.id]
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (Id::new(i), node))
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl<T> Index<Id<T>> for Nodes<T> {
    type Output = T;

    fn index(&self, id: Id<T>) -> &T {
        &self.nodes[id.id]
    }
}

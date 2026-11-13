use std::ops::Index;

use som_base::{Arena, Id, Span};

/// An arena of syntax nodes, each with the span it was parsed from.
pub struct Nodes<T> {
    nodes: Arena<T>,
    spans: Vec<Span>,
}

impl<T> Default for Nodes<T> {
    fn default() -> Self {
        Self {
            nodes: Arena::new(),
            spans: Vec::new(),
        }
    }
}

impl<T> Nodes<T> {
    pub fn alloc(&mut self, node: T, span: Span) -> Id<T> {
        self.spans.push(span);
        self.nodes.alloc(node)
    }

    pub fn span(&self, id: Id<T>) -> &Span {
        &self.spans[id.id]
    }

    /// Replaces a node and its span, keeping its id.
    pub fn replace(&mut self, id: Id<T>, node: T, span: Span) {
        self.nodes[id] = node;
        self.spans[id.id] = span;
    }

    pub fn get_mut(&mut self, id: Id<T>) -> &mut T {
        &mut self.nodes[id]
    }

    pub fn iter(&self) -> impl Iterator<Item = (Id<T>, &T)> {
        self.nodes.iter_with_ids()
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
        &self.nodes[id]
    }
}

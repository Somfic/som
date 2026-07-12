use std::any::Any;

use som_common::GenId;

use crate::node::{Event, Handler, Node, Tag};

pub trait Renderer: Any {
    fn create_element(&mut self, tag: Tag) -> GenId<Node>;
    fn create_text(&mut self) -> GenId<Node>;

    fn insert(&mut self, parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>);
    fn remove(&mut self, node: GenId<Node>);

    fn set_text(&mut self, node: GenId<Node>, text: &str);
    fn set_attr(&mut self, node: GenId<Node>, name: &str, value: &str);
    fn remove_attr(&mut self, node: GenId<Node>, name: &str);
    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool);

    fn listen(&mut self, node: GenId<Node>, event: Event, handler: GenId<Handler>);
}

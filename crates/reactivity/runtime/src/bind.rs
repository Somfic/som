use som_canvas::{Attribute, Event, Node, Tag};
use som_common::GenId;
use som_reactive::effect;

use crate::handler::register;
use crate::renderer::with_renderer;

pub fn create_element(tag: Tag) -> GenId<Node> {
    with_renderer(|r| r.create_element(tag))
}

pub fn create_text() -> GenId<Node> {
    with_renderer(|r| r.create_text())
}

pub fn insert(parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>) {
    with_renderer(|r| r.insert(parent, child, before));
}

pub fn set_attr(node: GenId<Node>, name: Attribute, value: &str) {
    with_renderer(|r| r.set_attr(node, name, value));
}

pub fn bind_text(node: GenId<Node>, f: impl Fn() -> String + 'static) {
    effect(move || {
        let text = f();
        with_renderer(|r| r.set_text(node, &text));
    });
}

pub fn bind_class(node: GenId<Node>, class: impl Into<String>, f: impl Fn() -> bool + 'static) {
    let class = class.into();
    effect(move || {
        let on = f();
        with_renderer(|r| r.set_class(node, &class, on));
    });
}

pub fn on(node: GenId<Node>, event: Event, handler: impl FnMut() + 'static) {
    let id = register(handler);
    with_renderer(|r| r.listen(node, event, id));
}

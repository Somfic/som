use som_canvas::{Node, Tag};
use som_common::GenId;
use som_reactive::effect;

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

pub fn set_attr(node: GenId<Node>, name: &str, value: &str) {
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

pub fn on(_node: GenId<Node>, _event: &str, _handler: impl FnMut() + 'static) {
    todo!("handler registration + dispatch — pending HandlerId design")
}

use std::cell::Cell;
use std::ptr;

use som_canvas::{Attribute, Event, Handler, Node, Renderer, Tag};
use som_common::GenId;

use crate::renderer::BlitzRenderer;

thread_local! {
    static ACTIVE: Cell<*mut BlitzRenderer> = const { Cell::new(ptr::null_mut()) };
}

pub(crate) fn set_active(renderer: &mut BlitzRenderer) {
    ACTIVE.with(|c| c.set(renderer));
}

pub(crate) fn clear_active() {
    ACTIVE.with(|c| c.set(ptr::null_mut()));
}

fn with_active<R>(f: impl FnOnce(&mut BlitzRenderer) -> R) -> R {
    ACTIVE.with(|c| {
        let p = c.get();
        assert!(
            !p.is_null(),
            "the blitz renderer was used outside a build/dispatch scope"
        );
        // SAFETY: `ACTIVE` is set only while a `&mut BlitzRenderer` owned by a
        // live `SomView` is parked (build() and handle_ui_event()), single
        // threaded and non-reentrant, so no other reference aliases it.
        unsafe { f(&mut *p) }
    })
}

pub struct AmbientBlitz;

impl Renderer for AmbientBlitz {
    fn create_element(&mut self, tag: Tag) -> GenId<Node> {
        with_active(|r| r.create_element(tag))
    }
    fn create_text(&mut self) -> GenId<Node> {
        with_active(|r| r.create_text())
    }
    fn insert(&mut self, parent: GenId<Node>, child: GenId<Node>, before: Option<GenId<Node>>) {
        with_active(|r| r.insert(parent, child, before))
    }
    fn remove(&mut self, node: GenId<Node>) {
        with_active(|r| r.remove(node))
    }
    fn set_text(&mut self, node: GenId<Node>, text: &str) {
        with_active(|r| r.set_text(node, text))
    }
    fn set_attr(&mut self, node: GenId<Node>, name: Attribute, value: &str) {
        with_active(|r| r.set_attr(node, name, value))
    }
    fn remove_attr(&mut self, node: GenId<Node>, name: Attribute) {
        with_active(|r| r.remove_attr(node, name))
    }
    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool) {
        with_active(|r| r.set_class(node, class, on))
    }
    fn listen(&mut self, node: GenId<Node>, event: Event, handler: GenId<Handler>) {
        with_active(|r| r.listen(node, event, handler))
    }
}

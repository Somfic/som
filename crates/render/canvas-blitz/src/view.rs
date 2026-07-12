use std::any::Any;
use std::ops::{Deref, DerefMut};

use blitz_dom::{BaseDocument, Document, DocumentMutator, EventDriver, EventHandler};
use blitz_traits::events::{DomEvent, DomEventData, EventState, UiEvent};
use som_canvas::{Event, Handler, Node};
use som_common::{GenArena, GenId};

use crate::ambient::{clear_active, set_active};
use crate::renderer::BlitzRenderer;

pub struct SomView {
    renderer: Box<BlitzRenderer>,
    on_dispatch: Box<dyn Fn(GenId<Handler>)>,
}

impl Deref for SomView {
    type Target = BaseDocument;
    fn deref(&self) -> &BaseDocument {
        &self.renderer.doc
    }
}

impl DerefMut for SomView {
    fn deref_mut(&mut self) -> &mut BaseDocument {
        &mut self.renderer.doc
    }
}

impl Document for SomView {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn handle_ui_event(&mut self, event: UiEvent) {
        let mut fired: Vec<GenId<Handler>> = Vec::new();
        {
            let r = &mut *self.renderer;
            let handlers = &r.handlers;
            let nodes = &r.nodes;
            let collect = CollectHandler {
                handlers,
                nodes,
                fired: &mut fired,
            };
            let mut driver = EventDriver::new(DocumentMutator::new(&mut r.doc), collect);
            driver.handle_ui_event(event);
        }

        if !fired.is_empty() {
            set_active(&mut self.renderer);
            for handler in fired {
                (self.on_dispatch)(handler);
            }
            clear_active();
        }
    }
}

struct CollectHandler<'a> {
    handlers: &'a [(GenId<Node>, Event, GenId<Handler>)],
    nodes: &'a GenArena<usize>,
    fired: &'a mut Vec<GenId<Handler>>,
}

impl EventHandler for CollectHandler<'_> {
    fn handle_event(
        &mut self,
        chain: &[usize],
        event: &mut DomEvent,
        _mutr: &mut DocumentMutator<'_>,
        _state: &mut EventState,
    ) {
        if !matches!(event.data, DomEventData::Click(_)) {
            return;
        }
        for &blitz_node in chain {
            for (node, ev, handler) in self.handlers {
                if *ev == Event::Click && self.nodes.get(node.cast()) == Some(&blitz_node) {
                    self.fired.push(*handler);
                }
            }
        }
    }
}

pub fn build(
    build: impl FnOnce(GenId<Node>),
    on_dispatch: impl Fn(GenId<Handler>) + 'static,
) -> SomView {
    let mut renderer = Box::new(BlitzRenderer::new());
    let root = renderer.root();
    set_active(&mut renderer);
    build(root);
    clear_active();
    SomView {
        renderer,
        on_dispatch: Box::new(on_dispatch),
    }
}

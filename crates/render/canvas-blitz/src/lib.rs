use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::ptr;

use blitz_dom::{
    BaseDocument, DEFAULT_CSS, Document, DocumentConfig, DocumentMutator, EventDriver, EventHandler,
    LocalName, QualName, local_name, ns,
};
use blitz_traits::events::{DomEvent, DomEventData, EventState, UiEvent};
use som_canvas::{Event, Handler, Node, Renderer, Tag};
use som_common::{GenArena, GenId};

/// Owns a Blitz document and the `GenId<Node> -> usize` map. Implements the
/// canvas [`Renderer`] port directly; used standalone (tests) and, behind the
/// [`SomView`] hand-off, as the target the ambient renderer mutates.
pub struct BlitzRenderer {
    doc: BaseDocument,
    nodes: GenArena<usize>,
    classes: HashMap<usize, Vec<String>>,
    handlers: Vec<(GenId<Node>, Event, GenId<Handler>)>,
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

        // A bare BaseDocument has only the document node (0). Blitz lays out and
        // paints from an <html> root element, so scaffold <html><body> and mount
        // the app tree under <body>.
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

    /// The mount point (the `<body>`); mount the app tree under this.
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
        self.handlers.push((node, event, handler));
    }
}

// ---- Ambient bridge -------------------------------------------------------
//
// The runtime installs a single `Box<dyn Renderer>` and drives it during both
// build and reactive updates. But the live document must be owned by the
// blitz-shell window ([`SomView`]) so it can be painted and receive events, and
// the `Document` trait is `Deref`-based (so no `Rc<RefCell>`). We bridge with a
// thread-local raw pointer to the active `BlitzRenderer`, set only for the
// duration of a synchronous build/dispatch window, and a zero-sized proxy
// ([`AmbientBlitz`]) that forwards each call to it.

thread_local! {
    static ACTIVE: Cell<*mut BlitzRenderer> = const { Cell::new(ptr::null_mut()) };
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

/// Zero-sized renderer installed into the runtime; forwards to the active
/// [`BlitzRenderer`] behind [`ACTIVE`].
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
    fn set_attr(&mut self, node: GenId<Node>, name: &str, value: &str) {
        with_active(|r| r.set_attr(node, name, value))
    }
    fn remove_attr(&mut self, node: GenId<Node>, name: &str) {
        with_active(|r| r.remove_attr(node, name))
    }
    fn set_class(&mut self, node: GenId<Node>, class: &str, on: bool) {
        with_active(|r| r.set_class(node, class, on))
    }
    fn listen(&mut self, node: GenId<Node>, event: Event, handler: GenId<Handler>) {
        with_active(|r| r.listen(node, event, handler))
    }
}

// ---- The window's Document ------------------------------------------------

/// A built document owned by a blitz-shell window. Routes clicks to registered
/// handlers via the supplied dispatch callback.
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
            // Park a pointer to the renderer so reactive effects triggered by
            // dispatch can mutate this document, then run the handlers.
            ACTIVE.with(|c| c.set(&mut *self.renderer));
            for handler in fired {
                (self.on_dispatch)(handler);
            }
            ACTIVE.with(|c| c.set(ptr::null_mut()));
        }
    }
}

/// Collects the handlers registered for clicked nodes (including ancestors in
/// the event chain), deferring dispatch until the doc borrow is released.
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
                if *ev == Event::Click
                    && self.nodes.get(node.cast()) == Some(&blitz_node)
                {
                    self.fired.push(*handler);
                }
            }
        }
    }
}

/// Build an app tree via the canvas [`Renderer`] port and produce a window-ready
/// [`SomView`]. The `build` closure runs the caller's `rt.*` calls (which reach
/// the ambient [`AmbientBlitz`], so install it into the runtime first). Clicks
/// are routed through `on_dispatch`.
pub fn build(
    build: impl FnOnce(GenId<Node>),
    on_dispatch: impl Fn(GenId<Handler>) + 'static,
) -> SomView {
    let mut renderer = Box::new(BlitzRenderer::new());
    let root = renderer.root();
    ACTIVE.with(|c| c.set(&mut *renderer));
    build(root);
    ACTIVE.with(|c| c.set(ptr::null_mut()));
    SomView {
        renderer,
        on_dispatch: Box::new(on_dispatch),
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

    // Headless proof of the reactive->renderer->document seam (the part the live
    // window exercises), without a GPU/winit window. Drives a signal through
    // `bind_text` and asserts the Blitz text node actually changed — routed via
    // the ACTIVE pointer + AmbientBlitz, exactly as during a real click.
    #[test]
    fn reactive_update_mutates_the_document() {
        use som_runtime::{bind_text, create_text, insert, install, signal};

        install(Box::new(AmbientBlitz));

        let mut renderer = Box::new(BlitzRenderer::new());
        let root = renderer.root();
        ACTIVE.with(|c| c.set(&mut *renderer));

        let n = signal(0);
        let text = create_text();
        insert(root, text, None);
        bind_text(text, move || format!("Count: {}", n.get()));

        let text_bid = renderer.blitz_id(text);
        let read = |r: &BlitzRenderer| r.doc.get_node(text_bid).unwrap().text_content();

        assert_eq!(read(&renderer), "Count: 0"); // initial effect run
        n.set(1);
        assert_eq!(read(&renderer), "Count: 1"); // reactive update reached the DOM
        n.set(2);
        assert_eq!(read(&renderer), "Count: 2");

        ACTIVE.with(|c| c.set(ptr::null_mut()));
    }
}

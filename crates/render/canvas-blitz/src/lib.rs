mod ambient;
mod renderer;
mod view;

pub use ambient::AmbientBlitz;
pub use renderer::BlitzRenderer;
pub use view::{SomView, build};

#[cfg(test)]
mod tests {
    use blitz_dom::DocumentMutator;
    use som_canvas::{Renderer, Tag};

    use crate::AmbientBlitz;
    use crate::ambient::{clear_active, set_active};
    use crate::renderer::BlitzRenderer;

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

    #[test]
    fn reactive_update_mutates_the_document() {
        use som_runtime::{bind_text, create_text, insert, install, signal};

        install(Box::new(AmbientBlitz));

        let mut renderer = Box::new(BlitzRenderer::new());
        let root = renderer.root();
        set_active(&mut renderer);

        let n = signal(0);
        let text = create_text();
        insert(root, text, None);
        bind_text(text, move || format!("Count: {}", n.get()));

        let text_bid = renderer.blitz_id(text);
        let read = |r: &BlitzRenderer| r.doc.get_node(text_bid).unwrap().text_content();

        assert_eq!(read(&renderer), "Count: 0");
        n.set(1);
        assert_eq!(read(&renderer), "Count: 1");
        n.set(2);
        assert_eq!(read(&renderer), "Count: 2");

        clear_active();
    }
}

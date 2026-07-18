mod ambient;
mod renderer;
mod view;

pub use ambient::AmbientBlitz;
pub use renderer::BlitzRenderer;
pub use view::{SomView, build};

use anyrender_vello::VelloWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, WindowConfig, create_default_event_loop};
use som_canvas::Node;
use som_common::GenId;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// Install the ambient Blitz renderer, build the UI via `build_fn`, open a
/// window, and run the event loop until it closes. Blocks the calling thread.
pub fn run(build_fn: impl FnOnce(GenId<Node>)) {
    som_runtime::install(Box::new(AmbientBlitz));
    let view = build(build_fn, som_runtime::dispatch);

    let event_loop = create_default_event_loop::<BlitzShellEvent>();
    let proxy = event_loop.create_proxy();
    let mut application = CleanExit(BlitzApplication::new(proxy));
    let window = WindowConfig::new(Box::new(view) as _, VelloWindowRenderer::new());
    application.0.add_window(window);
    event_loop.run_app(&mut application).unwrap();
}

/// Wraps `BlitzApplication` so window close terminates the process cleanly.
/// winit's macOS teardown throws an `NSException` on close (a TouchBar observer
/// bug, upstream — not our code); exiting before it runs avoids the abort.
struct CleanExit(BlitzApplication<VelloWindowRenderer>);

impl ApplicationHandler<BlitzShellEvent> for CleanExit {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.0.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.0.suspended(event_loop);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: BlitzShellEvent) {
        self.0.user_event(event_loop, event);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if matches!(event, WindowEvent::CloseRequested) {
            std::process::exit(0);
        }
        self.0.window_event(event_loop, window_id, event);
    }
}

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

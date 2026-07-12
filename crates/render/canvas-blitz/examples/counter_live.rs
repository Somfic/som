//! Increment 3: a fully interactive counter — built through our `rt.*` runtime,
//! painted by Blitz, and clickable. Still hand-written Rust (no som language),
//! but every layer of the stack is now exercised end to end:
//!
//!   click -> blitz-shell -> SomView::handle_ui_event -> runtime::dispatch
//!         -> handler sets the signal -> bind_text effect -> renderer.set_text
//!         -> document mutated -> repaint.

use anyrender_vello::VelloWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, WindowConfig, create_default_event_loop};
use som_canvas::{Event, Tag};
use som_canvas_blitz::{AmbientBlitz, build};
use som_runtime::{bind_text, create_element, create_text, dispatch, insert, on, set_attr, signal};

fn main() {
    // The runtime drives the ambient Blitz renderer.
    som_runtime::install(Box::new(AmbientBlitz));

    let view = build(
        |body| {
            set_attr(
                body,
                "style",
                "display:flex;flex-direction:column;align-items:center;justify-content:center;\
                 min-height:100vh;margin:0;gap:20px;background:#1e1e2e;color:#cdd6f4;\
                 font-family:sans-serif;",
            );

            let count = signal(0);

            let main = create_element(Tag::Main);
            insert(body, main, None);

            let label = create_text();
            insert(main, label, None);
            bind_text(label, move || format!("Count: {}", count.get()));

            let button = create_element(Tag::Button);
            set_attr(
                button,
                "style",
                "font-size:20px;padding:10px 24px;border:none;border-radius:10px;\
                 background:#89b4fa;color:#1e1e2e;",
            );
            insert(main, button, None);
            on(button, Event::Click, move || count.set(count.get() + 1));

            let plus = create_text();
            insert(button, plus, None);
            bind_text(plus, || "+1".to_string());
        },
        dispatch,
    );

    let event_loop = create_default_event_loop::<BlitzShellEvent>();
    let proxy = event_loop.create_proxy();
    let mut application = BlitzApplication::new(proxy);
    let window = WindowConfig::new(Box::new(view) as _, VelloWindowRenderer::new());
    application.add_window(window);
    event_loop.run_app(&mut application).unwrap();
}

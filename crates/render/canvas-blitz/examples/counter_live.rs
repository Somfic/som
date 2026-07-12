use anyrender_vello::VelloWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, WindowConfig, create_default_event_loop};
use som_canvas::{Renderer, Tag};
use som_canvas_blitz::BlitzRenderer;

fn main() {
    let mut r = BlitzRenderer::new();
    let body = r.root();

    r.set_attr(
        body,
        "style",
        "display:flex;flex-direction:column;align-items:center;justify-content:center;\
         min-height:100vh;margin:0;gap:20px;background:#1e1e2e;color:#cdd6f4;font-family:sans-serif;",
    );

    let main = r.create_element(Tag::Main);
    r.insert(body, main, None);

    let count = r.create_text();
    r.insert(main, count, None);
    r.set_text(count, "Count: 0");

    let button = r.create_element(Tag::Button);
    r.set_attr(
        button,
        "style",
        "font-size:20px;padding:10px 24px;border:none;border-radius:10px;\
         background:#89b4fa;color:#1e1e2e;",
    );
    r.insert(main, button, None);
    let label = r.create_text();
    r.insert(button, label, None);
    r.set_text(label, "+1");

    let doc = r.into_document();

    let event_loop = create_default_event_loop::<BlitzShellEvent>();
    let proxy = event_loop.create_proxy();
    let mut application = BlitzApplication::new(proxy);
    let window = WindowConfig::new(Box::new(doc) as _, VelloWindowRenderer::new());
    application.add_window(window);
    event_loop.run_app(&mut application).unwrap();
}

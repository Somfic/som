pub use som_reactive::*;

mod bind;
mod handler;
mod renderer;

pub use bind::*;
pub use handler::dispatch;
pub use renderer::{install, take, with_renderer};

#[cfg(test)]
mod tests {
    use std::any::Any;

    use som_canvas::{Event, MockRenderer, Tag};

    use super::*;

    #[test]
    fn bind_text_is_an_effect_that_writes_the_renderer() {
        install(Box::new(MockRenderer::new()));

        let n = signal(0);
        let t = create_text();
        bind_text(t, move || format!("Count: {}", n.get()));

        n.set(1);

        let renderer = take().unwrap();
        let any: &dyn Any = &*renderer;
        let mock = any.downcast_ref::<MockRenderer>().unwrap();

        assert_eq!(
            mock.log(),
            &[
                "create_text() -> #0",
                r#"set_text(text#0, "Count: 0")"#,
                r#"set_text(text#0, "Count: 1")"#,
            ]
        );
    }

    #[test]
    fn click_dispatch_drives_reactive_update() {
        install(Box::new(MockRenderer::new()));

        let n = signal(0);
        let root = create_element(Tag::Main);
        let count = create_text();
        insert(root, count, None);
        bind_text(count, move || format!("Count: {}", n.get()));
        on(root, Event::Click, move || n.set(n.get() + 1));

        let click = with_renderer(|r| {
            let any: &dyn Any = &*r;
            any.downcast_ref::<MockRenderer>()
                .unwrap()
                .handler_for(root, Event::Click)
                .unwrap()
        });

        dispatch(click);
        dispatch(click);

        let renderer = take().unwrap();
        let any: &dyn Any = &*renderer;
        let mock = any.downcast_ref::<MockRenderer>().unwrap();

        assert_eq!(
            mock.log(),
            &[
                r#"create_element("main") -> #0"#,
                "create_text() -> #1",
                "insert(parent=main#0, child=text#1, before=None)",
                r#"set_text(text#1, "Count: 0")"#,
                r#"listen(main#0, "click", handler=#0)"#,
                r#"set_text(text#1, "Count: 1")"#,
                r#"set_text(text#1, "Count: 2")"#,
            ]
        );
    }
}

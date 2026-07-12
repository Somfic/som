pub use som_reactive::*;

mod bind;
mod renderer;

pub use bind::*;
pub use renderer::{install, take, with_renderer};

#[cfg(test)]
mod tests {
    use std::any::Any;

    use som_canvas::MockRenderer;

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
}

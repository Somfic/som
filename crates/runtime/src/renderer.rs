use std::cell::RefCell;

use som_canvas::Renderer;

thread_local! {
    static RENDERER: RefCell<Option<Box<dyn Renderer>>> = const { RefCell::new(None) };
}

pub fn install(renderer: Box<dyn Renderer>) {
    RENDERER.with(|slot| *slot.borrow_mut() = Some(renderer));
}

pub fn take() -> Option<Box<dyn Renderer>> {
    RENDERER.with(|slot| slot.borrow_mut().take())
}

pub fn with_renderer<T>(f: impl FnOnce(&mut dyn Renderer) -> T) -> T {
    RENDERER.with(|slot| {
        let mut guard = slot.borrow_mut();
        let renderer = guard
            .as_deref_mut()
            .expect("no renderer installed; call runtime::install first");
        f(renderer)
    })
}

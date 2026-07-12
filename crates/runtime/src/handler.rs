use std::cell::RefCell;
use std::mem;

use som_canvas::Handler;
use som_common::{GenArena, GenId};

struct HandlerEntry {
    run: Box<dyn FnMut()>,
}

thread_local! {
    static HANDLERS: RefCell<GenArena<HandlerEntry>> = RefCell::new(GenArena::new());
}

pub fn register(f: impl FnMut() + 'static) -> GenId<Handler> {
    HANDLERS.with(|h| {
        let id = h.borrow_mut().insert(HandlerEntry { run: Box::new(f) });
        id.cast::<Handler>()
    })
}

pub fn dispatch(handler: GenId<Handler>) {
    let id = handler.cast::<HandlerEntry>();

    let taken = HANDLERS.with(|h| {
        h.borrow_mut()
            .get_mut(id)
            .map(|entry| mem::replace(&mut entry.run, Box::new(|| {})))
    });

    let mut run = match taken {
        Some(run) => run,
        None => return,
    };

    run();

    HANDLERS.with(|h| {
        if let Some(entry) = h.borrow_mut().get_mut(id) {
            entry.run = run;
        }
    });
}

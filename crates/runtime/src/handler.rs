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

// TODO(phase 6/7): handlers are not scope-owned yet, so `dispose` doesn't drop
// them — harmless now (dispatch is generational-stale-safe and nothing churns
// handlers), but becomes load-bearing once branch/each tear down subtrees. Wire
// this up via an `on_cleanup(f)` hook in reactive's scope system that registers
// removal of this entry.
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

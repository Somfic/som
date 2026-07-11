use crate::{Slot, with_runtime};
use som_common::Id;
use std::collections::HashSet;

pub(crate) struct Computation {
    pub dependencies: HashSet<Id<Slot>>,
    pub run: Box<dyn FnMut()>,
}

impl Computation {
    pub(crate) fn new(run: Box<dyn FnMut()>) -> Self {
        Computation {
            dependencies: HashSet::new(),
            run,
        }
    }
}

pub(crate) fn run_computation(id: Id<Computation>) {
    let mut run = with_runtime(|rt| {
        // unsubscribe from the old dependencies
        let old_deps = rt.computations[id].dependencies.clone();
        for slot_id in old_deps {
            rt.slots[slot_id].subscribers.remove(&id);
        }
        rt.computations[id].dependencies.clear();

        // run
        rt.push_running(id);
        std::mem::replace(&mut rt.computations[id].run, Box::new(|| {}))
    });

    run(); // outside of runtime borrow, since it will need to borrow runtime on its own

    with_runtime(|rt| {
        // clean up after running
        rt.computations[id].run = run;
        rt.pop_running();
    });
}

use crate::{Slot, with_runtime};
use som_common::GenId;
use std::collections::HashSet;

pub(crate) struct Computation {
    pub dependencies: HashSet<GenId<Slot>>,
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

pub(crate) fn run_computation(id: GenId<Computation>) {
    let taken = with_runtime(|rt| {
        if !rt.computations.contains(id) {
            return None;
        }

        let old_deps: Vec<GenId<Slot>> = rt.computations[id].dependencies.drain().collect();
        for slot_id in old_deps {
            if let Some(slot) = rt.slots.get_mut(slot_id) {
                slot.subscribers.remove(&id);
            }
        }

        rt.push_running(id);
        Some(std::mem::replace(
            &mut rt.computations[id].run,
            Box::new(|| {}),
        ))
    });

    let Some(mut run) = taken else { return };

    run(); // outside of runtime borrow, since it will need to borrow runtime on its own

    with_runtime(|rt| {
        if let Some(comp) = rt.computations.get_mut(id) {
            comp.run = run;
        }
        rt.pop_running();
    });
}

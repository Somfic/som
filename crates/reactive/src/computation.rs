use crate::{ComputationKey, SlotKey, with_runtime};
use std::collections::HashSet;

pub(crate) struct Computation {
    pub dependencies: HashSet<SlotKey>,
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

pub(crate) fn run_computation(id: ComputationKey) {
    let taken = with_runtime(|rt| {
        if !rt.computations.contains_key(id) {
            return None;
        }

        let old_deps: Vec<SlotKey> = rt.computations[id].dependencies.drain().collect();
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

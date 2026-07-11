use crate::{Slot, run_computation, with_runtime};
use som_common::Id;
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub struct Signal<T> {
    id: Id<Slot>,
    _marker: PhantomData<T>,
}

impl<T> Signal<T> {
    pub(crate) fn new(id: Id<Slot>) -> Self {
        Signal {
            id,
            _marker: PhantomData,
        }
    }
}

impl<T: Clone + 'static> Signal<T> {
    pub fn get(self) -> T {
        with_runtime(|rt| {
            let slot = &mut rt.slots[self.id];

            let value = slot.value.downcast_ref::<T>().unwrap().clone();

            // subscribe the current computation to this signal
            if let Some(current) = rt.running.last() {
                slot.subscribers.insert(*current);
                rt.computations[*current].dependencies.insert(self.id);
            }

            value
        })
    }

    pub fn set(self, value: T) {
        let subscribers: Vec<_> = with_runtime(|rt| {
            rt.slots[self.id].value = Box::new(value);
            rt.slots[self.id].subscribers.iter().copied().collect()
        });

        for computation in subscribers {
            run_computation(computation);
        }
    }
}

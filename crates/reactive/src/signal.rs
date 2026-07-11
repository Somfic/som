use crate::{SlotKey, run_computation, with_runtime};
use std::marker::PhantomData;

#[derive(Clone, Copy)]
pub struct Signal<T> {
    id: SlotKey,
    _marker: PhantomData<T>,
}

impl<T> Signal<T> {
    pub(crate) fn new(id: SlotKey) -> Self {
        Signal {
            id,
            _marker: PhantomData,
        }
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// Read the value, subscribing the currently-running computation.
    ///
    /// Returns `None` if the slot has been freed (its scope was disposed).
    /// Because slot keys are generational, a stale handle can never read a
    /// reused slot of a different type — the lookup simply misses.
    pub fn try_get(self) -> Option<T> {
        with_runtime(|rt| {
            let current = rt.running.last().copied();

            let slot = rt.slots.get_mut(self.id)?;
            let value = slot.value.downcast_ref::<T>().unwrap().clone();

            // subscribe the current computation to this signal
            if let Some(current) = current {
                slot.subscribers.insert(current);
                rt.computations[current].dependencies.insert(self.id);
            }

            Some(value)
        })
    }

    pub fn get(self) -> T {
        self.try_get()
            .expect("signal read after its scope was disposed")
    }

    pub fn set(self, value: T) {
        let subscribers: Vec<_> = with_runtime(|rt| {
            // Setting a disposed signal is a no-op: nothing is subscribed to it.
            let Some(slot) = rt.slots.get_mut(self.id) else {
                return Vec::new();
            };
            slot.value = Box::new(value);
            slot.subscribers.iter().copied().collect()
        });

        for computation in subscribers {
            run_computation(computation);
        }
    }
}

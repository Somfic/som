mod computation;
mod runtime;
mod signal;

pub use signal::*;

use crate::{
    computation::{Computation, run_computation},
    runtime::{Slot, with_runtime},
};

pub fn signal<T: Clone + 'static>(value: T) -> Signal<T> {
    let slot = with_runtime(|rt| {
        let slot = Slot::new(Box::new(value));
        rt.slots.alloc(slot)
    });

    Signal::new(slot)
}

pub fn effect(f: impl FnMut() + 'static) {
    let computation = with_runtime(|rt| {
        let computation = Computation::new(Box::new(f));
        rt.computations.alloc(computation)
    });

    // run once at the start
    run_computation(computation);
}

pub fn derived<T: Clone + 'static>(f: impl Fn() -> T + 'static) -> Signal<T> {
    let value = f();

    let slot = with_runtime(|rt| {
        let slot = Slot::new(Box::new(value));
        rt.slots.alloc(slot)
    });

    let computation = with_runtime(|rt| {
        let computation = Computation::new(Box::new(move || {
            let value = f(); // TODO: this reruns twice on first run, optimise
            Signal::new(slot).set(value);
        }));

        rt.computations.alloc(computation)
    });

    // run once at start
    run_computation(computation);

    Signal::new(slot)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{derived, effect, signal};

    #[test]
    fn effect_fires_on_change() {
        // "effect fires on change"
        let n = signal(0);
        let log = Rc::new(RefCell::new(vec![]));
        {
            let log = log.clone();
            effect(move || log.borrow_mut().push(n.get()));
        }
        assert_eq!(*log.borrow(), [0]); // ran once immediately

        n.set(1);
        n.set(2);
        assert_eq!(*log.borrow(), [0, 1, 2]); // re-ran on each change
    }

    #[test]
    fn no_spurious_reruns() {
        let x = signal(0);
        let y = signal(0);
        let runs = Rc::new(RefCell::new(0));
        {
            let runs = runs.clone();
            effect(move || {
                *runs.borrow_mut() += 1;
                x.get();
            });
        }
        y.set(9);
        assert_eq!(*runs.borrow(), 1);
    }

    #[test]
    fn basic_propagation() {
        let n = signal(0);
        let d = derived(move || n.get() * 2);
        assert_eq!(d.get(), 0);
        n.set(5);
        assert_eq!(d.get(), 10);
    }

    #[test]
    fn chained_propagation() {
        let n = signal(1);
        let a = derived(move || n.get() + 1);
        let b = derived(move || a.get() * 10);
        assert_eq!(b.get(), 20);
        n.set(2);
        assert_eq!(b.get(), 30);
    }

    #[test]
    fn conditonal_dependencies() {
        let toggle = signal(true);
        let a = signal("A");
        let b = signal("B");
        let log = Rc::new(RefCell::new(vec![]));
        {
            let (toggle, a, b, log) = (toggle, a, b, log.clone());
            effect(move || {
                log.borrow_mut()
                    .push(if toggle.get() { a.get() } else { b.get() });
            });
        }
        b.set("B2");
        toggle.set(false);
        a.set("A2");
        b.set("B3");

        assert_eq!(*log.borrow(), ["A", "B2", "B3"]);
    }
}

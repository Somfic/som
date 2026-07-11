use crate::{
    computation::{Computation, run_computation},
    runtime::{Slot, with_runtime},
};
pub use scope::*;
pub use signal::*;

mod computation;
mod runtime;
mod scope;
mod signal;

pub fn signal<T: Clone + 'static>(value: T) -> Signal<T> {
    let slot = with_runtime(|rt| {
        let slot = rt.slots.insert(Slot::new(Box::new(value)));
        let scope = rt.current_scope();
        rt.scopes[scope].slots.push(slot);
        slot
    });

    Signal::new(slot)
}

pub fn effect(f: impl FnMut() + 'static) {
    let computation = with_runtime(|rt| {
        let computation = rt.computations.insert(Computation::new(Box::new(f)));
        let scope = rt.current_scope();
        rt.scopes[scope].computations.push(computation);
        computation
    });

    // run once at the start
    run_computation(computation);
}

pub fn derived<T: Clone + 'static>(f: impl Fn() -> T + 'static) -> Signal<T> {
    let value = f();

    let slot = with_runtime(|rt| {
        let slot = rt.slots.insert(Slot::new(Box::new(value)));
        let scope = rt.current_scope();
        rt.scopes[scope].slots.push(slot);
        slot
    });

    let computation = with_runtime(|rt| {
        let computation = rt.computations.insert(Computation::new(Box::new(move || {
            let value = f(); // TODO: this reruns twice on first run, optimise
            Signal::new(slot).set(value);
        })));
        let scope = rt.current_scope();
        rt.scopes[scope].computations.push(computation);
        computation
    });

    // run once at start
    run_computation(computation);

    Signal::new(slot)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{Signal, derived, effect, scope, signal};

    #[test]
    fn effect_fires_on_change() {
        let n = signal(0);
        let log = Rc::new(RefCell::new(vec![]));
        {
            let log = log.clone();
            effect(move || log.borrow_mut().push(n.get()));
        }
        assert_eq!(*log.borrow(), [0]);

        n.set(1);
        n.set(2);
        assert_eq!(*log.borrow(), [0, 1, 2]);
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

    #[test]
    fn disposed_scope_stops_firing() {
        let n = signal(0);
        let log = Rc::new(RefCell::new(vec![]));

        let s = scope();
        {
            let log = log.clone();
            s.run(|| {
                effect(move || log.borrow_mut().push(n.get()));
            });
        }
        assert_eq!(*log.borrow(), [0]);

        n.set(1);
        assert_eq!(*log.borrow(), [0, 1]);

        s.dispose();
        n.set(2);
        n.set(3);
        assert_eq!(*log.borrow(), [0, 1]);
    }

    #[test]
    fn disposing_parent_disposes_children() {
        let n = signal(0);
        let log = Rc::new(RefCell::new(vec![]));

        let parent = scope();
        parent.run(|| {
            let child = scope();
            let log = log.clone();
            child.run(|| {
                effect(move || log.borrow_mut().push(n.get()));
            });
        });
        assert_eq!(*log.borrow(), [0]);

        n.set(1);
        assert_eq!(*log.borrow(), [0, 1]);

        parent.dispose();
        n.set(2);
        assert_eq!(*log.borrow(), [0, 1]);
    }

    #[test]
    fn stale_signal_handle_returns_none() {
        let s = scope();
        let a: Signal<&str> = s.run(|| signal("hello"));
        assert_eq!(a.try_get(), Some("hello"));

        s.dispose();

        let _b = signal(42_i32);
        assert_eq!(a.try_get(), None);
    }

    #[test]
    fn set_on_disposed_signal_is_a_noop() {
        let s = scope();
        let a: Signal<i32> = s.run(|| signal(1));
        s.dispose();
        a.set(99);
        assert_eq!(a.try_get(), None);
    }
}

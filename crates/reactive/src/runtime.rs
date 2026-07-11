use crate::Computation;
use som_common::{Arena, Id};
use std::{any::Any, cell::RefCell, collections::HashSet};

pub(crate) struct Runtime {
    pub running: Vec<Id<Computation>>,
    pub computations: Arena<Computation>,
    pub slots: Arena<Slot>,
}

pub(crate) struct Slot {
    pub value: Box<dyn Any>,
    pub subscribers: HashSet<Id<Computation>>,
}

impl Slot {
    pub fn new(value: Box<dyn Any>) -> Self {
        Slot {
            value,
            subscribers: HashSet::new(),
        }
    }
}

impl Runtime {
    pub(crate) fn new() -> Self {
        Runtime {
            running: Vec::new(),
            computations: Arena::default(),
            slots: Arena::default(),
        }
    }

    pub(crate) fn push_running(&mut self, id: Id<Computation>) {
        self.running.push(id);
    }

    pub(crate) fn pop_running(&mut self) {
        self.running.pop();
    }
}

thread_local! {
    static RUNTIME: RefCell<Runtime> = RefCell::new(Runtime::new());
}

pub(crate) fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|cell| {
        let mut rt = cell.borrow_mut();
        f(&mut rt)
    })
}

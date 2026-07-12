use crate::Computation;
use som_common::{GenArena, GenId};
use std::{any::Any, cell::RefCell, collections::HashSet};

pub(crate) struct Slot {
    pub value: Box<dyn Any>,
    pub subscribers: HashSet<GenId<Computation>>,
}

impl Slot {
    pub fn new(value: Box<dyn Any>) -> Self {
        Slot {
            value,
            subscribers: HashSet::new(),
        }
    }
}

pub(crate) struct ScopeData {
    pub parent: Option<GenId<ScopeData>>,
    pub children: Vec<GenId<ScopeData>>,
    pub computations: Vec<GenId<Computation>>,
    pub slots: Vec<GenId<Slot>>,
}

impl ScopeData {
    pub(crate) fn new(parent: Option<GenId<ScopeData>>) -> Self {
        ScopeData {
            parent,
            children: Vec::new(),
            computations: Vec::new(),
            slots: Vec::new(),
        }
    }
}

pub(crate) struct Runtime {
    pub running: Vec<GenId<Computation>>,
    pub scope_stack: Vec<GenId<ScopeData>>,
    pub computations: GenArena<Computation>,
    pub slots: GenArena<Slot>,
    pub scopes: GenArena<ScopeData>,
}

impl Runtime {
    pub(crate) fn new() -> Self {
        let mut scopes = GenArena::new();
        let root = scopes.insert(ScopeData::new(None));
        Runtime {
            running: Vec::new(),
            scope_stack: vec![root],
            computations: GenArena::new(),
            slots: GenArena::new(),
            scopes,
        }
    }

    pub(crate) fn current_scope(&self) -> GenId<ScopeData> {
        *self.scope_stack.last().expect("scope stack is never empty")
    }

    pub(crate) fn push_running(&mut self, id: GenId<Computation>) {
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

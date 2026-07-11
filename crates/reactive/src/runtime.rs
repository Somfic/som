use crate::Computation;
use slotmap::{SlotMap, new_key_type};
use std::{any::Any, cell::RefCell, collections::HashSet};

new_key_type! {
    /// Handle to a signal's value cell. Generational: once a slot is freed,
    /// any old key pointing at it is invalidated, so a stale read returns
    /// `None` from the slotmap instead of silently reading a reused slot.
    pub struct SlotKey;
    /// Handle to a computation (effect / derived body).
    pub struct ComputationKey;
    /// Handle to a disposal scope.
    pub struct ScopeKey;
}

pub(crate) struct Slot {
    pub value: Box<dyn Any>,
    pub subscribers: HashSet<ComputationKey>,
}

impl Slot {
    pub fn new(value: Box<dyn Any>) -> Self {
        Slot {
            value,
            subscribers: HashSet::new(),
        }
    }
}

/// A disposal scope: owns every computation and slot created while it was the
/// active scope, plus any child scopes. Disposing it tears all of them down.
pub(crate) struct ScopeData {
    pub parent: Option<ScopeKey>,
    pub children: Vec<ScopeKey>,
    pub computations: Vec<ComputationKey>,
    pub slots: Vec<SlotKey>,
}

impl ScopeData {
    pub(crate) fn new(parent: Option<ScopeKey>) -> Self {
        ScopeData {
            parent,
            children: Vec::new(),
            computations: Vec::new(),
            slots: Vec::new(),
        }
    }
}

pub(crate) struct Runtime {
    /// Stack of currently-running computations, for dynamic dependency tracking.
    pub running: Vec<ComputationKey>,
    /// Stack of active scopes; the top owns newly-created computations/slots.
    /// Always non-empty — the root scope sits at the bottom and is never popped.
    pub scope_stack: Vec<ScopeKey>,
    pub computations: SlotMap<ComputationKey, Computation>,
    pub slots: SlotMap<SlotKey, Slot>,
    pub scopes: SlotMap<ScopeKey, ScopeData>,
}

impl Runtime {
    pub(crate) fn new() -> Self {
        let mut scopes = SlotMap::with_key();
        let root = scopes.insert(ScopeData::new(None));
        Runtime {
            running: Vec::new(),
            scope_stack: vec![root],
            computations: SlotMap::with_key(),
            slots: SlotMap::with_key(),
            scopes,
        }
    }

    /// The scope that currently owns newly-created computations and slots.
    pub(crate) fn current_scope(&self) -> ScopeKey {
        *self.scope_stack.last().expect("scope stack is never empty")
    }

    pub(crate) fn push_running(&mut self, id: ComputationKey) {
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

use crate::{runtime::ScopeData, with_runtime};
use som_common::GenId;

#[derive(Clone, Copy)]
pub struct Scope {
    key: GenId<ScopeData>,
}

pub fn scope() -> Scope {
    with_runtime(|rt| {
        let parent = rt.current_scope();
        let key = rt.scopes.insert(ScopeData::new(Some(parent)));
        rt.scopes[parent].children.push(key);
        Scope { key }
    })
}

impl Scope {
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        with_runtime(|rt| rt.scope_stack.push(self.key));

        let _guard = ScopeGuard;

        f()
    }

    pub fn dispose(self) {
        dispose_scope(self.key);
    }
}

struct ScopeGuard;

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        with_runtime(|rt| {
            rt.scope_stack.pop();
        });
    }
}

fn dispose_scope(key: GenId<ScopeData>) {
    let (children, computations, slots, parent) = match with_runtime(|rt| {
        rt.scopes.get(key).map(|s| {
            (
                s.children.clone(),
                s.computations.clone(),
                s.slots.clone(),
                s.parent,
            )
        })
    }) {
        Some(parts) => parts,
        None => return,
    };

    for child in children {
        dispose_scope(child);
    }

    with_runtime(|rt| {
        for c in computations {
            if let Some(comp) = rt.computations.remove(c) {
                for slot_id in comp.dependencies {
                    if let Some(slot) = rt.slots.get_mut(slot_id) {
                        slot.subscribers.remove(&c);
                    }
                }
            }
        }

        for s in slots {
            rt.slots.remove(s);
        }

        if let Some(parent) = parent
            && let Some(p) = rt.scopes.get_mut(parent)
        {
            p.children.retain(|&k| k != key);
        }
        rt.scopes.remove(key);
    });
}

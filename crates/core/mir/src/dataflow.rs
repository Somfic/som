use crate::{BlockId, Body};
use std::collections::VecDeque;

/// Each block's state on entry, `None` for one never reached: `start` at the first block,
/// then what `transfer` makes of each block, merged into its successors by `join` until
/// nothing changes. `join` merges what flows in and says whether that changed anything.
pub fn forward<S: Clone>(
    body: &Body,
    start: S,
    mut transfer: impl FnMut(BlockId, &mut S),
    mut join: impl FnMut(&mut S, &S) -> bool,
) -> Vec<Option<S>> {
    let mut entry: Vec<Option<S>> = vec![None; body.blocks.len()];
    entry[0] = Some(start);
    let mut queue = VecDeque::from([BlockId::new(0)]);
    while let Some(block) = queue.pop_front() {
        let mut state = entry[block.id].clone().expect("queued blocks have a state");
        transfer(block, &mut state);
        for next in body.blocks[block].terminator.kind.successors() {
            let changed = match &mut entry[next.id] {
                Some(existing) => join(existing, &state),
                slot @ None => {
                    *slot = Some(state.clone());
                    true
                }
            };
            if changed && !queue.contains(&next) {
                queue.push_back(next);
            }
        }
    }
    entry
}

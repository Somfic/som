//! The interpreter that runs one initializer: [`call`] steps through statements and calls,
//! [`place`] reads and writes memory, and [`value`] works out rvalues.

mod call;
mod place;
mod value;

use crate::limits::STEP_LIMIT;
use crate::memory::Byte;
use crate::{Loc, State, Value, foreign};
use som_base::Span;
use som_mir::*;
use std::collections::HashMap;

pub(crate) struct Frame {
    pub func: FuncId,
    pub locals: Vec<Value>,
    pub block: BlockId,
    pub stmt: usize,
    /// Where what it returns goes, in the frame that called it, and its type.
    pub dest: Option<(Loc, TypeId)>,
    pub call_span: Span,
}

pub(crate) struct Machine<'s, 'm> {
    pub state: &'s mut State<'m>,
    pub frames: Vec<Frame>,
    pub steps: u64,
    /// The most steps it runs for: `STEP_LIMIT`, or fewer when only trying a call.
    pub limit: u64,
    /// Memory allocated while running: each allocation's bytes, `None` once it's freed.
    pub heap: Vec<Option<Vec<Byte>>>,
    /// The allocations C has been given, each with its real buffer: see [`foreign`].
    pub pinned: HashMap<usize, foreign::Pin>,
}

/// What can't be kept of a constant's value, and where in it: `[".mem", ".ptr"]`.
pub(crate) type Unkeepable = (&'static str, Vec<String>);

impl<'s, 'm> Machine<'s, 'm> {
    pub fn new(state: &'s mut State<'m>) -> Machine<'s, 'm> {
        Machine {
            state,
            frames: Vec::new(),
            steps: 0,
            limit: STEP_LIMIT,
            heap: Vec::new(),
            pinned: HashMap::new(),
        }
    }

    pub fn mir(&self) -> &'m Mir {
        self.state.mir
    }
}

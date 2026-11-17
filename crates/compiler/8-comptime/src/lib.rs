//! Works out each `@const` whose value runs code, by running its initializer while compiling,
//! then puts the value it gets in the program in place of every `ConstKind::Pending`.
//!
//! Runs after the borrow checker, so the code it runs has passed every check. It's a small
//! interpreter over MIR: each function call a frame of locals, each value a [`Value`]. It
//! does what the program would do when it runs: integers wrap to their type's width, `f32`
//! rounds to `f32` at every step, and `usize` is the host's width, like the C backend's.
//! Memory it allocates is bytes, laid out as C lays them out (see [`memory`]), and the C
//! functions the standard library's text uses (`memcpy`, `strlen`, `snprintf`, …) are done
//! here, in Rust (see [`natives`]). Other extern functions are called for real (see
//! [`foreign`]).

mod error;
mod foreign;
mod limits;
mod machine;
mod memory;
mod natives;
mod numbers;
mod printf;
mod suggest;

pub use error::{ComptimeError, Working};
pub use limits::STEP_LIMIT;
pub use suggest::{CouldBeConst, could_be_const};

use machine::Machine;
use som_base::Span;
use som_lang::Builtin;
use som_mir::*;
use std::collections::HashMap;

/// Works out initializers `inits`: each one's value, or why it can't be worked out. For a
/// constant needed before the program's types can be checked: `mir` need only have what
/// they run.
pub fn evaluate(mir: &Mir, inits: &[FuncId]) -> (Vec<Option<ConstKind>>, Vec<ComptimeError>) {
    let mut state = State::new(mir);
    let values = inits.iter().map(|&init| state.init(init).ok()).collect();
    (values, deduplicated(state.errors))
}

/// Works out every initializer the program uses, and puts their values in its place. If
/// one can't be worked out, says why, and leaves the program as it was.
pub fn run(mir: &mut Mir) -> Vec<ComptimeError> {
    // `@const assert(…);`, each run for what it checks; then each constant something uses.
    let mut wanted = mir.checks.clone();
    for func in mir.funcs.iter() {
        if let Some(body) = &func.body {
            for op in body.operands() {
                if let Operand::Const(Const {
                    kind: ConstKind::Pending(init),
                    ..
                }) = op
                {
                    wanted.push(*init);
                }
            }
        }
    }
    let mut state = State::new(mir);
    for init in wanted {
        let _ = state.init(init);
    }
    let State {
        results, errors, ..
    } = state;
    let errors = deduplicated(errors);
    if !errors.is_empty() {
        return errors;
    }
    for func in mir.funcs.iter_mut() {
        let Some(body) = &mut func.body else {
            continue;
        };
        for op in body.operands_mut() {
            if let Operand::Const(c) = op
                && let ConstKind::Pending(init) = c.kind
                && let Some(Some(value)) = results.get(&init)
            {
                c.kind = value.clone();
            }
        }
    }
    errors
}

/// The same error, said the same way, once: copies of an `@for` around items share where
/// they're written.
fn deduplicated(errors: Vec<ComptimeError>) -> Vec<ComptimeError> {
    let mut kept: Vec<ComptimeError> = Vec::new();
    for e in errors {
        if !kept.contains(&e) {
            kept.push(e);
        }
    }
    kept
}

struct State<'m> {
    mir: &'m Mir,
    /// Each initializer worked out: its value, or `None` if it couldn't be (and was reported).
    results: HashMap<FuncId, Option<ConstKind>>,
    /// The initializers being worked out, outermost first.
    running: Vec<FuncId>,
    errors: Vec<ComptimeError>,
}

impl<'m> State<'m> {
    fn new(mir: &'m Mir) -> State<'m> {
        State {
            mir,
            results: HashMap::new(),
            running: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// What calling `func` with `args`, each a constant, gives while compiling, and how many
    /// steps it took: `None` if it can't be worked out within `limit` steps, or what it gives
    /// can't be put in the program.
    fn try_call(
        mir: &'m Mir,
        func: FuncId,
        args: &[Operand],
        limit: u64,
    ) -> Option<(ConstKind, u64)> {
        let mut state = State::new(mir);
        let mut machine = Machine::new(&mut state);
        let span = mir.funcs[func].span.clone();
        let values = (args.iter())
            .map(|a| machine.operand(a, &span))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        machine.limit = limit;
        let value = machine.run_with(func, values).ok()?;
        let steps = machine.steps;
        let kind = machine.bake(&value, mir.funcs[func].ret).ok()?;
        Some((kind, steps))
    }

    /// The value of initializer `init`, working it out the first time.
    fn init(&mut self, init: FuncId) -> Result<ConstKind, Stop> {
        if let Some(result) = self.results.get(&init) {
            return result.clone().ok_or(Stop::Quiet);
        }
        if self.running.contains(&init) {
            return Err(Stop::Report(
                Fault::Cycle(const_name(self.mir, init)),
                self.mir.funcs[init].span.clone(),
            ));
        }
        self.running.push(init);
        let mir = self.mir;
        let mut machine = Machine::new(self);
        let result = machine.run(init);
        let trace = machine.trace();
        // Said where the program goes into the standard library, not where in it.
        let result = result.map_err(|stop| match stop {
            Stop::Report(fault, span) => Stop::Report(fault, machine.blame(span)),
            stop => stop,
        });
        let trace = trace[machine.std_frames()..].to_vec();
        // Its value, while the memory it might point to is still there.
        let result = result.map(|value| machine.bake(&value, mir.funcs[init].ret));
        self.running.pop();
        let result = match result {
            Ok(Ok(kind)) => Some(kind),
            Ok(Err((what, at))) => {
                let name = const_name(mir, init);
                self.errors.push(ComptimeError::ConstHoldsMemory {
                    part: (!at.is_empty()).then(|| format!("{name}{}", at.concat())),
                    name,
                    what,
                    span: mir.funcs[init].span.clone(),
                });
                None
            }
            Err(Stop::Report(fault, span)) => {
                self.errors.push(fault.into_error(span, trace));
                None
            }
            Err(Stop::Quiet) => None,
        };
        self.results.insert(init, result.clone());
        result.ok_or(Stop::Quiet)
    }
}

/// The name of the `@const` initializer `init` works out: `BUFFER`.
fn const_name(mir: &Mir, init: FuncId) -> String {
    let name = &mir.funcs[init].name;
    let named = (name.strip_prefix(Builtin::Const.name())).and_then(|rest| rest.strip_prefix(' '));
    named.unwrap_or(name).to_string()
}

fn is_std(name: &str) -> bool {
    (name.strip_prefix(som_lang::PRELUDE_MODULE)).is_some_and(|rest| rest.starts_with("::"))
}

fn c_name(function: &Function) -> String {
    function.link_name.clone().unwrap_or(function.name.clone())
}

#[derive(Clone, Debug, PartialEq)]
enum Value {
    /// A local before it's assigned, or after its scope ends; memory never written.
    Uninit,
    Int(i128),
    Float(f64),
    Bool(bool),
    Unit,
    Func(FuncId),
    /// A struct's, enum's, array's, optional's or range's parts, in order.
    Agg(Vec<Value>),
    /// A reference, or a pointer: where it points.
    Ref(Loc),
    Null,
    /// A reference to a slice: where its first element is (`None` for an empty one with
    /// nowhere to start), and how many there are.
    Slice {
        start: Option<Loc>,
        len: usize,
    },
}

/// Where a value is: a local, then a path through its parts; or a byte in the heap.
#[derive(Clone, Debug, PartialEq)]
struct Loc {
    root: Root,
    /// For a local, the parts on the way to it. Empty in the heap, where the offset says
    /// where it is.
    path: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
enum Root {
    Local {
        frame: usize,
        local: Local,
    },
    Heap {
        alloc: usize,
        offset: usize,
    },
    /// A real address, of memory C gave: `getenv`'s text, `fopen`'s `FILE`.
    Foreign {
        address: usize,
    },
}

impl Loc {
    fn local(frame: usize, local: Local) -> Loc {
        Loc {
            root: Root::Local { frame, local },
            path: Vec::new(),
        }
    }

    fn heap(alloc: usize, offset: usize) -> Loc {
        Loc {
            root: Root::Heap { alloc, offset },
            path: Vec::new(),
        }
    }

    fn foreign(address: usize) -> Loc {
        Loc {
            root: Root::Foreign { address },
            path: Vec::new(),
        }
    }
}

/// Why running stopped: an error to report, at this span; or quietly, for one already reported.
enum Stop {
    Report(Fault, Span),
    Quiet,
}

enum Fault {
    CallsExtern(String),
    /// An extern function that can't be called while compiling, and why.
    Foreign(String, foreign::Missing),
    Unreachable,
    /// With `true`, `MIN / -1`.
    DividedByZero(bool),
    TooLong,
    Cycle(String),
    NotSupported(&'static str),
    OutOfBounds,
    Failed(String),
}

impl Fault {
    fn into_error(self, span: Span, trace: Vec<Working>) -> ComptimeError {
        match self {
            Fault::Foreign(function, missing) => ComptimeError::ConstCallsExtern {
                reason: missing.reason(),
                function,
                span,
                trace,
            },
            Fault::CallsExtern(function) => ComptimeError::ConstCallsExtern {
                reason: "an extern function only exists when the program runs".to_string(),
                function,
                span,
                trace,
            },
            Fault::Unreachable => ComptimeError::ConstReachedUnreachable { span, trace },
            Fault::DividedByZero(overflow) => ComptimeError::ConstDividedByZero {
                overflow,
                span,
                trace,
            },
            Fault::TooLong => ComptimeError::ConstTooLong { span, trace },
            Fault::Cycle(name) => ComptimeError::ConstCycle { name, span, trace },
            Fault::NotSupported(what) => ComptimeError::ConstNotSupported { what, span, trace },
            Fault::OutOfBounds => ComptimeError::ConstOutOfBounds { span, trace },
            Fault::Failed(message) => ComptimeError::ConstFailed {
                message,
                span,
                trace,
            },
        }
    }
}

fn unsupported<T>(what: &'static str, span: &Span) -> Result<T, Stop> {
    Err(Stop::Report(Fault::NotSupported(what), span.clone()))
}

fn out_of_bounds<T>(span: &Span) -> Result<T, Stop> {
    Err(Stop::Report(Fault::OutOfBounds, span.clone()))
}

/// Where pointer `value` points: `None` for null. A pointer never set is null: that's what
/// `som::mem::null` gives.
fn pointer(value: &Value, span: &Span) -> Result<Option<Loc>, Stop> {
    match value {
        Value::Ref(loc) => Ok(Some(loc.clone())),
        Value::Null | Value::Uninit => Ok(None),
        _ => unsupported("this kind of pointer", span),
    }
}

fn count(value: &Value, span: &Span) -> Result<usize, Stop> {
    match value {
        Value::Int(n) => usize::try_from(*n).or_else(|_| out_of_bounds(span)),
        _ => unsupported("a count that isn't a number", span),
    }
}

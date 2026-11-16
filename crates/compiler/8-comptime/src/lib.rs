//! Works out each `@const` whose value runs code, by running its initializer while compiling,
//! then puts the value it gets in the program in place of every `ConstKind::Pending`.
//!
//! Runs after the borrow checker, so the code it runs has passed every check. It's a small
//! interpreter over MIR: each function call a frame of locals, each value a [`Value`]. It
//! does what the program would do when it runs: integers wrap to their type's width, `f32`
//! rounds to `f32` at every step, and `usize` is the host's width, like the C backend's.
//! Memory it allocates is bytes, laid out as C lays them out (see [`memory`]), and the C
//! functions the standard library's text uses (`memcpy`, `strlen`, `snprintf`, …) are done
//! here, in Rust.

mod error;
mod memory;
mod printf;

pub use error::{ComptimeError, STEP_LIMIT, Working};

use memory::{Byte, POINTER, Unreadable, decode, encode, layout};
use som_base::Span;
use som_mir::*;
use std::collections::HashMap;

/// Works out every initializer the program uses, and puts their values in its place. If
/// one can't be worked out, says why, and leaves the program as it was.
pub fn run(mir: &mut Mir) -> Vec<ComptimeError> {
    // `@const assert(…);`, each run for what it checks; then each constant something uses.
    let mut wanted = mir.checks.clone();
    for func in mir.funcs.iter() {
        if let Some(body) = &func.body {
            for op in operands(body) {
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
    let mut state = State {
        mir,
        results: HashMap::new(),
        running: Vec::new(),
        errors: Vec::new(),
    };
    for init in wanted {
        let _ = state.init(init);
    }
    let State {
        results, errors, ..
    } = state;
    if !errors.is_empty() {
        return errors;
    }
    for func in mir.funcs.iter_mut() {
        let Some(body) = &mut func.body else {
            continue;
        };
        for op in operands_mut(body) {
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

/// What's known across every initializer.
struct State<'m> {
    mir: &'m Mir,
    /// Each initializer worked out: its value, or `None` if it couldn't be (and was reported).
    results: HashMap<FuncId, Option<ConstKind>>,
    /// The initializers being worked out, outermost first.
    running: Vec<FuncId>,
    errors: Vec<ComptimeError>,
}

impl State<'_> {
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
        let mut machine = Machine {
            state: self,
            frames: Vec::new(),
            steps: 0,
            heap: Vec::new(),
        };
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
    name.strip_prefix("@const ").unwrap_or(name).to_string()
}

/// A value while compiling.
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
    /// A null pointer.
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
    /// A frame's local.
    Local { frame: usize, local: Local },
    /// Byte `offset` of allocation `alloc`.
    Heap { alloc: usize, offset: usize },
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
}

/// Why running stopped: an error to report, at this span; or quietly, for one already reported.
enum Stop {
    Report(Fault, Span),
    Quiet,
}

enum Fault {
    CallsExtern(String),
    Unreachable,
    /// With `true`, `MIN / -1`.
    DividedByZero(bool),
    TooLong,
    Cycle(String),
    NotSupported(&'static str),
    OutOfBounds,
    /// `@fail`, with its message.
    Failed(String),
}

impl Fault {
    fn into_error(self, span: Span, trace: Vec<Working>) -> ComptimeError {
        match self {
            Fault::CallsExtern(function) => ComptimeError::ConstCallsExtern {
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

/// The number `value` is, as a count or an index.
fn count(value: &Value, span: &Span) -> Result<usize, Stop> {
    match value {
        Value::Int(n) => usize::try_from(*n).or_else(|_| out_of_bounds(span)),
        _ => unsupported("a count that isn't a number", span),
    }
}

/// The deepest calls can go while compiling.
const DEPTH_LIMIT: usize = 10_000;

/// One call of a function.
struct Frame {
    func: FuncId,
    locals: Vec<Value>,
    block: BlockId,
    /// The next statement in `block` to run.
    stmt: usize,
    /// Where what it returns goes, in the frame that called it, and its type.
    dest: Option<(Loc, TypeId)>,
    /// Where it was called.
    call_span: Span,
}

/// Runs one initializer.
struct Machine<'s, 'm> {
    state: &'s mut State<'m>,
    frames: Vec<Frame>,
    steps: u64,
    /// Memory allocated while running: each allocation's bytes, `None` once it's freed.
    heap: Vec<Option<Vec<Byte>>>,
}

/// What can't be kept of a constant's value, and where in it: `[".mem", ".ptr"]`.
type Unkeepable = (&'static str, Vec<String>);

impl Machine<'_, '_> {
    fn mir(&self) -> &Mir {
        self.state.mir
    }

    /// How running got where it is, from the innermost call out: each call, then the
    /// constant being worked out.
    fn trace(&self) -> Vec<Working> {
        let mut trace: Vec<Working> = (self.frames.iter().skip(1).rev())
            .map(|frame| Working {
                name: self.mir().funcs[frame.func].name.clone(),
                span: frame.call_span.clone(),
                call: true,
            })
            .collect();
        // One with no name is the line it's on: nothing to add.
        if let Some(first) = self.frames.first()
            && self.mir().funcs[first.func].name != "@const"
        {
            trace.push(Working {
                name: const_name(self.mir(), first.func),
                span: first.call_span.clone(),
                call: false,
            });
        }
        trace
    }

    /// How many of the innermost calls are the standard library's: `som::assert` failing is
    /// said where it's called.
    fn std_frames(&self) -> usize {
        (self.frames.iter().skip(1).rev())
            .take_while(|frame| self.mir().funcs[frame.func].name.starts_with("som::"))
            .count()
    }

    /// Where to say running stopped, given it stopped at `span`: there, or where the program
    /// called into the standard library.
    fn blame(&self, span: Span) -> Span {
        match self.std_frames() {
            0 => span,
            n => self.frames[self.frames.len() - n].call_span.clone(),
        }
    }

    /// Runs `init` to its end: the value it gives.
    fn run(&mut self, init: FuncId) -> Result<Value, Stop> {
        let span = self.mir().funcs[init].span.clone();
        self.push_frame(init, Vec::new(), None, span)?;
        loop {
            let top = self.frames.len() - 1;
            let (func, block, stmt) = {
                let frame = &self.frames[top];
                (frame.func, frame.block, frame.stmt)
            };
            let mir = self.state.mir;
            let body = mir.funcs[func].body.as_ref().expect("a body");
            let block = &body.blocks[block];
            self.steps += 1;
            if self.steps > STEP_LIMIT {
                let span = match block.stmts.get(stmt) {
                    Some(s) => s.span.clone(),
                    None => block.terminator.span.clone(),
                };
                return Err(Stop::Report(Fault::TooLong, span));
            }
            if let Some(statement) = block.stmts.get(stmt) {
                self.frames[top].stmt += 1;
                // `@fail` or `@unreachable`: stops here, an error while compiling. What the
                // program's `@on fail` functions would do only happens when it runs.
                if let StatementKind::Assign(_, Rvalue::Call { func, args }) = &statement.kind
                    && Some(*func) == mir.fail
                {
                    let span = &statement.span;
                    let failure = self.operand(&args[0], span)?;
                    return Err(Stop::Report(
                        self.failure(&failure, mir.funcs[*func].params[0], span)?,
                        span.clone(),
                    ));
                }
                self.statement(statement)?;
                continue;
            }
            let span = &block.terminator.span;
            match &block.terminator.kind {
                TerminatorKind::Goto(target) => self.jump(*target),
                TerminatorKind::Branch {
                    condition,
                    then,
                    otherwise,
                } => match self.operand(condition, span)? {
                    Value::Bool(true) => self.jump(*then),
                    Value::Bool(false) => self.jump(*otherwise),
                    _ => return unsupported("a condition that isn't a bool", span),
                },
                TerminatorKind::Return => {
                    let frame = self.frames.pop().expect("a frame");
                    let value = frame.locals[RETURN.id].clone();
                    match frame.dest {
                        _ if self.frames.is_empty() => return Ok(value),
                        Some((dest, ty)) => self.write(&dest, ty, value, span)?,
                        None => {}
                    }
                }
                TerminatorKind::Unreachable => {
                    return Err(Stop::Report(Fault::Unreachable, span.clone()));
                }
            }
        }
    }

    fn jump(&mut self, target: BlockId) {
        let frame = self.frames.last_mut().expect("a frame");
        frame.block = target;
        frame.stmt = 0;
    }

    /// Calls `func` with `args`, its result going to `dest`.
    fn push_frame(
        &mut self,
        func: FuncId,
        args: Vec<Value>,
        dest: Option<(Loc, TypeId)>,
        span: Span,
    ) -> Result<(), Stop> {
        let function = &self.mir().funcs[func];
        let Some(body) = &function.body else {
            let name = function.link_name.clone().unwrap_or(function.name.clone());
            // One of C's that's done here, or one that only exists when the program runs.
            let Some(value) = self.native(&name, &args, &span)? else {
                return Err(Stop::Report(Fault::CallsExtern(name), span));
            };
            if let Some((dest, ty)) = dest {
                self.write(&dest, ty, value, &span)?;
            }
            return Ok(());
        };
        if self.frames.len() >= DEPTH_LIMIT {
            return Err(Stop::Report(Fault::TooLong, span));
        }
        let mut locals = vec![Value::Uninit; body.locals.slots()];
        for (i, arg) in args.into_iter().enumerate() {
            locals[i + 1] = arg;
        }
        self.frames.push(Frame {
            func,
            locals,
            block: Id::new(0),
            stmt: 0,
            dest,
            call_span: span,
        });
        Ok(())
    }

    fn statement(&mut self, statement: &Statement) -> Result<(), Stop> {
        let span = &statement.span;
        let top = self.frames.len() - 1;
        match &statement.kind {
            StatementKind::Assign(place, Rvalue::Call { func, args }) => {
                let args = (args.iter())
                    .map(|a| self.operand(a, span))
                    .collect::<Result<Vec<_>, _>>()?;
                let dest = self.loc(place, span)?;
                self.push_frame(*func, args, Some(dest), span.clone())
            }
            StatementKind::Assign(place, rvalue) => {
                let value = self.rvalue(rvalue, span)?;
                let (dest, ty) = self.loc(place, span)?;
                self.write(&dest, ty, value, span)
            }
            StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                self.frames[top].locals[local.id] = Value::Uninit;
                Ok(())
            }
            StatementKind::Drop { place, glue, flag } => {
                if let Some(flag) = flag
                    && !matches!(self.frames[top].locals[flag.id], Value::Bool(true))
                {
                    return Ok(());
                }
                let (at, _) = self.loc(place, span)?;
                self.push_frame(*glue, vec![Value::Ref(at)], None, span.clone())
            }
        }
    }

    // --- Places ---

    /// Where `place` is, in the innermost frame, and its type.
    fn loc(&self, place: &Place, span: &Span) -> Result<(Loc, TypeId), Stop> {
        let frame = self.frames.len() - 1;
        let mir = self.mir();
        let body = mir.funcs[self.frames[frame].func]
            .body
            .as_ref()
            .expect("a body");
        let mut ty = body.locals[place.local].ty;
        let mut loc = Loc::local(frame, place.local);
        // Just past a slice's `*`: an index counts from where its elements start.
        let mut in_slice = false;
        for &projection in &place.projection {
            let projected = mir.project(ty, projection).expect("checked places");
            match projection {
                Projection::Field(i) => loc = self.part(loc, ty, i, span)?,
                Projection::Index(local) => {
                    let i = count(&self.frames[frame].locals[local.id], span)?;
                    loc = match in_slice {
                        true => self.step(&loc, i, projected, span)?,
                        false => self.part(loc, ty, i, span)?,
                    };
                }
                Projection::Deref => match self.read(&loc, ty, span)? {
                    Value::Ref(target) => loc = target,
                    Value::Slice {
                        start: Some(start), ..
                    } => {
                        loc = start;
                        in_slice = true;
                        ty = projected;
                        continue;
                    }
                    Value::Null | Value::Uninit | Value::Slice { start: None, .. } => {
                        return out_of_bounds(span);
                    }
                    _ => return unsupported("this kind of reference", span),
                },
            }
            ty = projected;
            in_slice = false;
        }
        Ok((loc, ty))
    }

    /// Part `i` of the value of type `ty` at `loc`: a field, or an array's element.
    fn part(&self, mut loc: Loc, ty: TypeId, i: usize, span: &Span) -> Result<Loc, Stop> {
        match &mut loc.root {
            Root::Local { .. } => loc.path.push(i),
            Root::Heap { offset, .. } => match layout(self.mir(), ty) {
                Some(layout) if i < layout.parts.len() => *offset += layout.parts[i].1,
                Some(_) => return out_of_bounds(span),
                None => return unsupported("memory holding a value of this type", span),
            },
        }
        Ok(loc)
    }

    /// `n` elements of type `element` on from `loc`: in its allocation, or its array.
    fn step(&self, loc: &Loc, n: usize, element: TypeId, span: &Span) -> Result<Loc, Stop> {
        let mut loc = loc.clone();
        match (&mut loc.root, loc.path.last_mut()) {
            (Root::Heap { offset, .. }, _) => match layout(self.mir(), element) {
                Some(layout) => *offset += n * layout.size,
                None => return unsupported("memory holding a value of this type", span),
            },
            (_, Some(last)) => *last += n,
            (Root::Local { .. }, None) if n == 0 => {}
            (Root::Local { .. }, None) => return out_of_bounds(span),
        }
        Ok(loc)
    }

    /// The bytes of allocation `alloc` from `offset`, `size` of them.
    fn bytes(
        &self,
        alloc: usize,
        offset: usize,
        size: usize,
        span: &Span,
    ) -> Result<&[Byte], Stop> {
        match self.heap.get(alloc) {
            Some(Some(bytes)) if offset + size <= bytes.len() => Ok(&bytes[offset..offset + size]),
            _ => out_of_bounds(span),
        }
    }

    /// The value of type `ty` at `loc`.
    fn read(&self, loc: &Loc, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        match &loc.root {
            Root::Local { frame, local } => {
                let mut value = &self.frames[*frame].locals[local.id];
                for &i in &loc.path {
                    value = match (value, i) {
                        (Value::Agg(parts), _) => parts.get(i).unwrap_or(&Value::Uninit),
                        (Value::Uninit, _) => &Value::Uninit,
                        // A slice's reference holds where its elements start, and how many.
                        (Value::Slice { start, .. }, 0) => {
                            return Ok(start.clone().map_or(Value::Null, Value::Ref));
                        }
                        (Value::Slice { len, .. }, 1) => return Ok(Value::Int(*len as i128)),
                        _ => return unsupported("this kind of value's parts", span),
                    };
                }
                Ok(value.clone())
            }
            Root::Heap { alloc, offset } => {
                let Some(layout) = layout(self.mir(), ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                let bytes = self.bytes(*alloc, *offset, layout.size, span)?;
                decode(self.mir(), bytes, ty).or_else(|e| match e {
                    Unreadable::Type => unsupported("memory holding a value of this type", span),
                    Unreadable::Pointer => unsupported("a pointer's bytes as something else", span),
                })
            }
        }
    }

    /// Puts `value`, of type `ty`, at `loc`.
    fn write(&mut self, loc: &Loc, ty: TypeId, value: Value, span: &Span) -> Result<(), Stop> {
        match &loc.root {
            Root::Local { frame, local } => {
                let mut slot = &mut self.frames[*frame].locals[local.id];
                for &i in &loc.path {
                    if matches!(slot, Value::Uninit) {
                        *slot = Value::Agg(Vec::new());
                    }
                    slot = match slot {
                        Value::Agg(parts) => {
                            if parts.len() <= i {
                                parts.resize(i + 1, Value::Uninit);
                            }
                            &mut parts[i]
                        }
                        _ => return unsupported("changing part of a slice's reference", span),
                    };
                }
                *slot = value;
                Ok(())
            }
            &Root::Heap { alloc, offset } => {
                let Some(bytes) = encode(self.mir(), &value, ty) else {
                    return unsupported("memory holding a value of this type", span);
                };
                match self.heap.get_mut(alloc) {
                    Some(Some(memory)) if offset + bytes.len() <= memory.len() => {
                        memory[offset..offset + bytes.len()].clone_from_slice(&bytes);
                        Ok(())
                    }
                    _ => out_of_bounds(span),
                }
            }
        }
    }

    /// A new allocation of `bytes`: where it starts.
    fn allocate(&mut self, bytes: Vec<Byte>) -> Loc {
        self.heap.push(Some(bytes));
        Loc::heap(self.heap.len() - 1, 0)
    }

    /// Byte `n` on from `loc`. In a local, that's element `n` of an array of bytes.
    fn byte(&self, loc: &Loc, n: usize, span: &Span) -> Result<Byte, Stop> {
        match &loc.root {
            &Root::Heap { alloc, offset } => Ok(self.bytes(alloc, offset + n, 1, span)?[0].clone()),
            Root::Local { .. } => {
                let at = self.step(loc, n, self.mir().types.u8, span)?;
                match self.read(&at, self.mir().types.u8, span)? {
                    Value::Int(b) => Ok(Byte::Data(b as u8)),
                    Value::Uninit => Ok(Byte::Uninit),
                    _ => unsupported("this kind of value as bytes", span),
                }
            }
        }
    }

    fn set_byte(&mut self, loc: &Loc, n: usize, byte: Byte, span: &Span) -> Result<(), Stop> {
        match &loc.root {
            &Root::Heap { alloc, offset } => match self.heap.get_mut(alloc) {
                Some(Some(memory)) if offset + n < memory.len() => {
                    memory[offset + n] = byte;
                    Ok(())
                }
                _ => out_of_bounds(span),
            },
            Root::Local { .. } => {
                let u8 = self.mir().types.u8;
                let at = self.step(loc, n, u8, span)?;
                let value = match byte {
                    Byte::Data(b) => Value::Int(b as i128),
                    Byte::Uninit => Value::Uninit,
                    Byte::Ptr(..) => return unsupported("a pointer's bytes in a local", span),
                };
                self.write(&at, u8, value, span)
            }
        }
    }

    /// The bytes of C text at `loc`, up to its zero byte.
    fn c_text(&self, loc: &Loc, span: &Span) -> Result<Vec<u8>, Stop> {
        let mut text = Vec::new();
        loop {
            match self.byte(loc, text.len(), span)? {
                Byte::Data(0) => return Ok(text),
                Byte::Data(b) => text.push(b),
                _ => return unsupported("text that isn't bytes", span),
            }
        }
    }

    /// What a `Failure` value says failed: `@unreachable`, or `@fail`'s message.
    fn failure(&self, failure: &Value, ty: TypeId, span: &Span) -> Result<Fault, Stop> {
        let (Value::Agg(parts), Ty::Struct(def, args)) = (failure, self.mir().types.get(ty)) else {
            return unsupported("this failure", span);
        };
        let fields = &self.mir().struct_def(*def, args).fields;
        let field =
            |name: &str| (fields.iter().position(|(n, _)| n == name)).and_then(|i| parts.get(i));
        if let Some(Value::Bool(true)) = field("unreachable") {
            return Ok(Fault::Unreachable);
        }
        match field("message") {
            Some(message) => Ok(Fault::Failed(self.string(message, span)?)),
            None => unsupported("this failure", span),
        }
    }

    /// The text a `string` value holds.
    fn string(&self, value: &Value, span: &Span) -> Result<String, Stop> {
        let Value::Agg(parts) = value else {
            return unsupported("this text", span);
        };
        let Some(Value::Ref(view)) = parts.first() else {
            return unsupported("this text", span);
        };
        let Root::Heap { alloc, offset } = view.root else {
            return unsupported("this text", span);
        };
        // The block: its count, its length, then its bytes.
        let block = self.data(view, 2 * POINTER, span)?;
        let mut word = [0u8; 16];
        word[..POINTER].copy_from_slice(&block[POINTER..]);
        let len = u128::from_le_bytes(word) as usize;
        let bytes = self.data(&Loc::heap(alloc, offset + 2 * POINTER), len, span)?;
        String::from_utf8(bytes).or_else(|_| unsupported("text that isn't UTF-8", span))
    }

    /// `n` bytes from `loc`, as data.
    fn data(&self, loc: &Loc, n: usize, span: &Span) -> Result<Vec<u8>, Stop> {
        (0..n)
            .map(|i| match self.byte(loc, i, span)? {
                Byte::Data(b) => Ok(b),
                Byte::Uninit => unsupported("memory before it's written", span),
                Byte::Ptr(..) => unsupported("a pointer's bytes as data", span),
            })
            .collect()
    }

    /// What intrinsic `intrinsic` gives, given its arguments: a pointer to `element`s, then
    /// the rest.
    fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        element: TypeId,
        args: Vec<Value>,
        span: &Span,
    ) -> Result<Value, Stop> {
        let p = pointer(&args[0], span)?;
        let size = match layout(self.mir(), element) {
            Some(layout) => layout.size,
            None => return unsupported("memory holding a value of this type", span),
        };
        // The `i`th element on from where `p` points.
        let at = |m: &Self, i: usize| match &p {
            Some(loc) => m.step(loc, i, element, span),
            None => out_of_bounds(span),
        };
        Ok(match intrinsic {
            Intrinsic::Resize => {
                let bytes = count(&args[1], span)? * size;
                match &p {
                    None => Value::Ref(self.allocate(vec![Byte::Uninit; bytes])),
                    Some(Loc {
                        root: Root::Heap { alloc, offset: 0 },
                        ..
                    }) => {
                        match self.heap.get_mut(*alloc) {
                            Some(Some(memory)) => memory.resize(bytes, Byte::Uninit),
                            _ => return out_of_bounds(span),
                        }
                        args[0].clone()
                    }
                    Some(_) => return unsupported("resizing memory it didn't allocate", span),
                }
            }
            Intrinsic::Free => {
                match &p {
                    None => {}
                    Some(Loc {
                        root: Root::Heap { alloc, offset: 0 },
                        ..
                    }) => match self.heap.get_mut(*alloc) {
                        Some(slot @ Some(_)) => *slot = None,
                        _ => return out_of_bounds(span),
                    },
                    Some(_) => return unsupported("freeing memory it didn't allocate", span),
                }
                Value::Unit
            }
            Intrinsic::Read => {
                let loc = at(self, count(&args[1], span)?)?;
                self.read(&loc, element, span)?
            }
            Intrinsic::Write => {
                let loc = at(self, count(&args[1], span)?)?;
                self.write(&loc, element, args[2].clone(), span)?;
                Value::Unit
            }
            Intrinsic::Offset => Value::Ref(at(self, count(&args[1], span)?)?),
            Intrinsic::View => Value::Slice {
                start: p.clone(),
                len: count(&args[1], span)?,
            },
            Intrinsic::IsNull => Value::Bool(p.is_none()),
        })
    }

    /// C function `name`, done here: its result, or `None` if it isn't one of these.
    fn native(&mut self, name: &str, args: &[Value], span: &Span) -> Result<Option<Value>, Stop> {
        let to = |value: &Value| match pointer(value, span)? {
            Some(loc) => Ok(loc),
            None => out_of_bounds(span),
        };
        Ok(Some(match name {
            "memcpy" | "memmove" => {
                let (dst, src, n) = (to(&args[0])?, to(&args[1])?, count(&args[2], span)?);
                // All read first: they can overlap.
                let bytes = (0..n)
                    .map(|i| self.byte(&src, i, span))
                    .collect::<Result<Vec<_>, _>>()?;
                for (i, byte) in bytes.into_iter().enumerate() {
                    self.set_byte(&dst, i, byte, span)?;
                }
                args[0].clone()
            }
            "memset" => {
                let (dst, n) = (to(&args[0])?, count(&args[2], span)?);
                let Value::Int(c) = args[1] else {
                    return unsupported("this byte", span);
                };
                for i in 0..n {
                    self.set_byte(&dst, i, Byte::Data(c as u8), span)?;
                }
                args[0].clone()
            }
            "memcmp" => {
                let n = count(&args[2], span)?;
                let a = self.data(&to(&args[0])?, n, span)?;
                let b = self.data(&to(&args[1])?, n, span)?;
                Value::Int(a.cmp(&b) as i128)
            }
            "strlen" => Value::Int(self.c_text(&to(&args[0])?, span)?.len() as i128),
            "snprintf" => {
                let (size, format) = (count(&args[1], span)?, self.c_text(&to(&args[2])?, span)?);
                let mut rest = Vec::new();
                for arg in &args[3..] {
                    rest.push(match arg {
                        Value::Int(n) => printf::Arg::Int(*n),
                        Value::Bool(b) => printf::Arg::Int(*b as i128),
                        Value::Float(x) => printf::Arg::Float(*x),
                        Value::Ref(loc) => printf::Arg::Text(self.c_text(loc, span)?),
                        _ => return unsupported("this argument to `snprintf`", span),
                    });
                }
                let Some(text) = printf::format(&format, &rest) else {
                    return unsupported("this `snprintf` format", span);
                };
                // As much as fits, then the zero byte.
                if size > 0 {
                    let dst = to(&args[0])?;
                    let shown = text.len().min(size - 1);
                    for (i, &b) in text[..shown].iter().enumerate() {
                        self.set_byte(&dst, i, Byte::Data(b), span)?;
                    }
                    self.set_byte(&dst, shown, Byte::Data(0), span)?;
                }
                Value::Int(text.len() as i128)
            }
            _ => return Ok(None),
        }))
    }

    // --- Values ---

    fn operand(&mut self, op: &Operand, span: &Span) -> Result<Value, Stop> {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                let (loc, ty) = self.loc(place, span)?;
                self.read(&loc, ty, span)
            }
            Operand::Const(c) => self.constant(c, span),
        }
    }

    fn constant(&mut self, c: &Const, span: &Span) -> Result<Value, Stop> {
        Ok(match &c.kind {
            ConstKind::Int(n) => Value::Int(*n),
            ConstKind::Float(x) => Value::Float(*x),
            ConstKind::Bool(b) => Value::Bool(*b),
            ConstKind::Str(s) => self.text(s, c.ty, span)?,
            ConstKind::Unit => Value::Unit,
            ConstKind::Func(f) => Value::Func(*f),
            ConstKind::Zeroed => self.zero(c.ty, span)?,
            ConstKind::Aggregate(parts) => Value::Agg(
                (parts.iter())
                    .map(|part| self.constant(part, span))
                    .collect::<Result<_, _>>()?,
            ),
            ConstKind::Pending(init) => {
                let kind = self.state.init(*init).map_err(|stop| match stop {
                    // Needing itself: reported where this one needs it.
                    Stop::Report(fault @ Fault::Cycle(_), _) => Stop::Report(fault, span.clone()),
                    stop => stop,
                })?;
                self.constant(&Const { kind, ty: c.ty }, span)?
            }
        })
    }

    /// A text literal of type `ty`, in memory as the program would have it: a `cstr`'s bytes
    /// and a zero byte; a `str`'s bytes; or a `string`'s block, with the count that says it's
    /// part of the program, its length, its bytes, and a zero byte.
    fn text(&mut self, text: &str, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let types = &self.mir().types;
        let (cstr, string) = (ty == types.cstr, ty == types.text);
        let str_ref =
            matches!(types.get(ty), Ty::Ref { to, .. } if matches!(types.get(*to), Ty::Str));
        let string = string && matches!(types.get(ty), Ty::Struct(..));
        let bytes: Vec<Byte> = text.bytes().map(Byte::Data).collect();
        let zero = [Byte::Data(0)];
        if cstr {
            return Ok(Value::Ref(self.allocate([&bytes[..], &zero].concat())));
        }
        if string {
            let word = |n: usize| (n as u128).to_le_bytes()[..POINTER].to_vec();
            let header: Vec<Byte> = [word(usize::MAX), word(text.len())]
                .concat()
                .into_iter()
                .map(Byte::Data)
                .collect();
            let view = self.allocate([&header[..], &bytes, &zero].concat());
            return Ok(Value::Agg(vec![Value::Ref(view)]));
        }
        if str_ref {
            let start = self.allocate(bytes);
            return Ok(Value::Slice {
                start: Some(start),
                len: text.len(),
            });
        }
        unsupported("text of this type", span)
    }

    /// A value of type `ty` with every byte zero.
    fn zero(&self, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let mir = self.mir();
        Ok(match mir.types.get(ty) {
            Ty::Unit => Value::Unit,
            Ty::Bool => Value::Bool(false),
            Ty::Char | Ty::Int(_) => Value::Int(0),
            Ty::Float(_) => Value::Float(0.0),
            Ty::Ref { to, .. } if matches!(mir.types.get(*to), Ty::Slice(_) | Ty::Str) => {
                Value::Slice {
                    start: None,
                    len: 0,
                }
            }
            Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. } | Ty::Fn { .. } => Value::Null,
            Ty::Struct(def, args) if mir.struct_def(*def, args).handle => Value::Null,
            Ty::Struct(..) | Ty::Optional(_) | Ty::Array { .. } | Ty::Range(_) => {
                let Some(layout) = layout(mir, ty) else {
                    return unsupported("a value of this type", span);
                };
                Value::Agg(
                    (layout.parts.iter())
                        .map(|&(ty, _)| self.zero(ty, span))
                        .collect::<Result<_, _>>()?,
                )
            }
            _ => return unsupported("a value of this type", span),
        })
    }

    fn array_len(&self, len: TypeId, span: &Span) -> Result<usize, Stop> {
        match self.mir().types.get(len) {
            Ty::Value(som_mir::Value::Int(n)) => Ok(*n as usize),
            _ => unsupported("an array of this length", span),
        }
    }

    fn operand_ty(&self, op: &Operand) -> TypeId {
        match op {
            Operand::Const(c) => c.ty,
            Operand::Copy(place) | Operand::Move(place) => {
                let frame = self.frames.last().expect("a frame");
                let body = self.mir().funcs[frame.func].body.as_ref().expect("a body");
                let mut ty = body.locals[place.local].ty;
                for &projection in &place.projection {
                    ty = self.mir().project(ty, projection).expect("checked places");
                }
                ty
            }
        }
    }

    fn rvalue(&mut self, rvalue: &Rvalue, span: &Span) -> Result<Value, Stop> {
        let operands = |m: &mut Self, ops: &[Operand]| -> Result<Vec<Value>, Stop> {
            ops.iter().map(|op| m.operand(op, span)).collect()
        };
        Ok(match rvalue {
            Rvalue::Use(op) => self.operand(op, span)?,
            // `&*r` is `r` again: for a slice, its start and its length.
            Rvalue::Ref { place, .. } if place.projection.last() == Some(&Projection::Deref) => {
                let mut inner = place.clone();
                inner.projection.pop();
                let (loc, ty) = self.loc(&inner, span)?;
                self.read(&loc, ty, span)?
            }
            Rvalue::Ref { place, .. } => Value::Ref(self.loc(place, span)?.0),
            Rvalue::Binary { op, lhs, rhs } => {
                let ty = self.operand_ty(lhs);
                let (a, b) = (self.operand(lhs, span)?, self.operand(rhs, span)?);
                self.binary(*op, a, b, ty, span)?
            }
            Rvalue::Not(op) => {
                let ty = self.operand_ty(op);
                match (self.operand(op, span)?, self.mir().types.get(ty)) {
                    (Value::Bool(b), _) => Value::Bool(!b),
                    (Value::Int(n), Ty::Int(int)) => Value::Int(wrap(!n, *int)),
                    _ => return unsupported("this `!`", span),
                }
            }
            Rvalue::Some(op) => Value::Agg(vec![Value::Bool(true), self.operand(op, span)?]),
            Rvalue::Array { items, .. } => Value::Agg(operands(self, items)?),
            Rvalue::Repeat { ty, operand } => {
                let value = self.operand(operand, span)?;
                let len = match self.mir().types.get(*ty) {
                    Ty::Array { len, .. } => self.array_len(*len, span)?,
                    _ => return unsupported("an array of this length", span),
                };
                Value::Agg(vec![value; len])
            }
            Rvalue::Range { start, end, .. } => {
                Value::Agg(vec![self.operand(start, span)?, self.operand(end, span)?])
            }
            Rvalue::Cast { operand, ty } => {
                let from = self.operand_ty(operand);
                let value = self.operand(operand, span)?;
                self.cast(value, from, *ty, span)?
            }
            Rvalue::Struct {
                variant: None,
                fields,
                ..
            } => Value::Agg(operands(self, fields)?),
            Rvalue::Struct {
                ty,
                variant: Some(variant),
                fields,
            } => {
                let Value::Agg(mut parts) = self.zero(*ty, span)? else {
                    return unsupported("this enum", span);
                };
                let Ty::Struct(def, args) = self.mir().types.get(*ty) else {
                    return unsupported("this enum", span);
                };
                let indexes = (self.mir().struct_def(*def, args))
                    .variant_fields(*variant)
                    .to_vec();
                // Its tag, first, says which variant it is.
                parts[0] = Value::Int(*variant as i128);
                for (index, value) in indexes.into_iter().zip(operands(self, fields)?) {
                    parts[index] = value;
                }
                Value::Agg(parts)
            }
            Rvalue::MakeSlice { start, len, .. } => {
                let start = pointer(&self.operand(start, span)?, span)?;
                let len = count(&self.operand(len, span)?, span)?;
                Value::Slice { start, len }
            }
            Rvalue::Intrinsic {
                intrinsic, args, ..
            } => {
                let element = match self.mir().types.get(self.operand_ty(&args[0])) {
                    Ty::Ptr(element) => *element,
                    _ => return unsupported("this pointer", span),
                };
                let args = operands(self, args)?;
                self.intrinsic(*intrinsic, element, args, span)?
            }
            Rvalue::Call { .. } => unreachable!("calls are statements of their own"),
        })
    }

    fn binary(
        &self,
        op: BinOp,
        a: Value,
        b: Value,
        ty: TypeId,
        span: &Span,
    ) -> Result<Value, Stop> {
        let ty = self.mir().types.get(ty);
        Ok(match (a, b, ty) {
            // `ptr + n`: `n` bytes on.
            (Value::Ref(mut loc), Value::Int(n), Ty::Pointer) if op == BinOp::Add => {
                match &mut loc.root {
                    Root::Heap { offset, .. } => *offset = offset.wrapping_add(n as usize),
                    Root::Local { .. } => return unsupported("bytes on from a local", span),
                }
                Value::Ref(loc)
            }
            (Value::Int(a), Value::Int(b), Ty::Int(int)) => match op {
                BinOp::Add => Value::Int(wrap(a.wrapping_add(b), *int)),
                BinOp::Subtract => Value::Int(wrap(a.wrapping_sub(b), *int)),
                BinOp::Multiply => Value::Int(wrap(a.wrapping_mul(b), *int)),
                BinOp::Divide | BinOp::Modulo if b == 0 || traps(a, b, *int) => {
                    let overflow = b != 0;
                    return Err(Stop::Report(Fault::DividedByZero(overflow), span.clone()));
                }
                // Both truncate towards zero.
                BinOp::Divide => Value::Int(wrap(a / b, *int)),
                BinOp::Modulo => Value::Int(wrap(a % b, *int)),
                BinOp::BitAnd => Value::Int(wrap(a & b, *int)),
                BinOp::BitOr => Value::Int(wrap(a | b, *int)),
                BinOp::BitXor => Value::Int(wrap(a ^ b, *int)),
                BinOp::ShiftLeft | BinOp::ShiftRight => Value::Int(shift(op, a, b, *int)),
                BinOp::LessThan => Value::Bool(a < b),
                BinOp::GreaterThan => Value::Bool(a > b),
                BinOp::LessThanOrEqual => Value::Bool(a <= b),
                BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                BinOp::And | BinOp::Or => return unsupported("this operator", span),
            },
            // A `char`'s compared as the number it is.
            (Value::Int(a), Value::Int(b), _) => match op {
                BinOp::LessThan => Value::Bool(a < b),
                BinOp::GreaterThan => Value::Bool(a > b),
                BinOp::LessThanOrEqual => Value::Bool(a <= b),
                BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                _ => return unsupported("this operator", span),
            },
            (Value::Float(a), Value::Float(b), Ty::Float(float)) => {
                let round = |x: f64| match float {
                    FloatTy::F32 => (x as f32) as f64,
                    FloatTy::F64 => x,
                };
                let (a, b) = (round(a), round(b));
                match op {
                    BinOp::Add => Value::Float(round(a + b)),
                    BinOp::Subtract => Value::Float(round(a - b)),
                    BinOp::Multiply => Value::Float(round(a * b)),
                    BinOp::Divide => Value::Float(round(a / b)),
                    BinOp::LessThan => Value::Bool(a < b),
                    BinOp::GreaterThan => Value::Bool(a > b),
                    BinOp::LessThanOrEqual => Value::Bool(a <= b),
                    BinOp::GreaterThanOrEqual => Value::Bool(a >= b),
                    BinOp::Equals => Value::Bool(a == b),
                    BinOp::NotEquals => Value::Bool(a != b),
                    _ => return unsupported("this operator on floats", span),
                }
            }
            // Pointers are equal when they point to the same place.
            (a @ (Value::Ref(_) | Value::Null), b @ (Value::Ref(_) | Value::Null), _)
                if matches!(op, BinOp::Equals | BinOp::NotEquals) =>
            {
                let same = match (&a, &b) {
                    (Value::Ref(a), Value::Ref(b)) => a == b,
                    (Value::Null, Value::Null) => true,
                    _ => false,
                };
                Value::Bool(same == (op == BinOp::Equals))
            }
            (Value::Bool(a), Value::Bool(b), _) => match op {
                BinOp::And => Value::Bool(a && b),
                BinOp::Or => Value::Bool(a || b),
                BinOp::Equals => Value::Bool(a == b),
                BinOp::NotEquals => Value::Bool(a != b),
                _ => return unsupported("this operator on bools", span),
            },
            _ => return unsupported("this operator", span),
        })
    }

    fn cast(&self, value: Value, from: TypeId, to: TypeId, span: &Span) -> Result<Value, Stop> {
        let types = &self.mir().types;
        let pointee = |t: TypeId| match types.get(t) {
            Ty::Ref { to, .. } => Some(types.get(*to)),
            _ => None,
        };
        // A reference to an array, as a slice of its elements.
        if let (Some(Ty::Array { len, .. }), Some(Ty::Slice(_))) = (pointee(from), pointee(to)) {
            let len = self.array_len(*len, span)?;
            return Ok(match value {
                Value::Ref(mut loc) => {
                    // In a local, its first element; in memory, it starts where it is.
                    if let Root::Local { .. } = loc.root {
                        loc.path.push(0);
                    }
                    Value::Slice {
                        start: Some(loc),
                        len,
                    }
                }
                _ => return unsupported("this conversion", span),
            });
        }
        // `[mut T]` as `[T]`, and pointers as other pointers: where they point is the same.
        let points = |t: &Ty| matches!(t, Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. });
        if points(types.get(from)) && points(types.get(to)) {
            return Ok(match value {
                Value::Uninit => Value::Null,
                value => value,
            });
        }
        Ok(match (value, types.get(from), types.get(to)) {
            (Value::Int(n), _, Ty::Int(int)) => Value::Int(wrap(n, *int)),
            (Value::Int(n), _, Ty::Float(float)) => Value::Float(round(n as f64, *float)),
            (Value::Int(n), _, Ty::Char) => Value::Int(n),
            (Value::Bool(b), _, Ty::Int(_)) => Value::Int(b as i128),
            // To the integer nearest zero, saturating; NaN is 0.
            (Value::Float(x), _, Ty::Int(int)) => {
                let (min, max) = bounds(*int);
                Value::Int(if x.is_nan() {
                    0
                } else if x <= min as f64 {
                    min
                } else if x >= max as f64 {
                    max
                } else {
                    x.trunc() as i128
                })
            }
            (Value::Float(x), _, Ty::Float(float)) => Value::Float(round(x, *float)),
            (value, a, b) if a == b => value,
            _ => return unsupported("this conversion", span),
        })
    }

    /// `value`, of type `ty`, as a constant in the program, or what it is that can't be one.
    /// Text built while compiling becomes a literal.
    fn bake(&self, value: &Value, ty: TypeId) -> Result<ConstKind, Unkeepable> {
        let mir = self.mir();
        let types = &mir.types;
        let span = &mir.funcs[self.frames.first().map_or(Id::new(0), |f| f.func)].span;
        let memory = || {
            (
                "a reference to memory that only exists while compiling",
                Vec::new(),
            )
        };
        // Text: its bytes, kept as a literal.
        let literal = |bytes: Result<Vec<u8>, Stop>| match bytes.ok().map(String::from_utf8) {
            Some(Ok(text)) => Ok(ConstKind::Str(text)),
            Some(Err(_)) => Err(("text that isn't UTF-8", Vec::new())),
            None => Err(memory()),
        };
        if ty == types.text && matches!(types.get(ty), Ty::Struct(..)) {
            return match self.string(value, span) {
                Ok(text) => Ok(ConstKind::Str(text)),
                Err(_) => Err(memory()),
            };
        }
        match (value, types.get(ty)) {
            (Value::Ref(loc), Ty::CStr) => return literal(self.c_text(loc, span)),
            (Value::Slice { start, len }, Ty::Ref { to, .. })
                if matches!(types.get(*to), Ty::Str) =>
            {
                return literal(match start {
                    Some(start) => self.data(start, *len, span),
                    None => Ok(Vec::new()),
                });
            }
            _ => {}
        }
        // Its parts, with their types, and how each is named.
        let parts = |parts: Vec<(&Value, TypeId, String)>| -> Result<ConstKind, Unkeepable> {
            let mut baked = Vec::new();
            for (value, ty, name) in parts {
                let kind = self.bake(value, ty).map_err(|(what, mut at)| {
                    at.insert(0, name);
                    (what, at)
                })?;
                baked.push(Const { kind, ty });
            }
            Ok(ConstKind::Aggregate(baked))
        };
        const UNSET: &Value = &Value::Uninit;
        fn nth(values: &[Value], i: usize) -> &Value {
            values.get(i).unwrap_or(UNSET)
        }
        Ok(match value {
            Value::Int(n) => ConstKind::Int(*n),
            Value::Float(x) if x.is_finite() => ConstKind::Float(*x),
            Value::Float(_) => return Err(("infinite, or not a number", Vec::new())),
            Value::Bool(b) => ConstKind::Bool(*b),
            Value::Unit => ConstKind::Unit,
            Value::Func(f) => ConstKind::Func(*f),
            // Never set: a field of a variant it isn't, or `none`'s value.
            Value::Uninit | Value::Null => ConstKind::Zeroed,
            Value::Agg(values) => match types.get(ty) {
                Ty::Struct(def, args) if !mir.struct_def(*def, args).handle => {
                    let s = mir.struct_def(*def, args);
                    parts(
                        (s.fields.iter().enumerate())
                            .map(|(i, (name, ty))| (nth(values, i), *ty, format!(".{name}")))
                            .collect(),
                    )?
                }
                Ty::Optional(inner) => parts(vec![
                    (nth(values, 0), types.bool, String::new()),
                    (nth(values, 1), *inner, String::new()),
                ])?,
                Ty::Array { element, .. } => parts(
                    (values.iter().enumerate())
                        .map(|(i, v)| (v, *element, format!("[{i}]")))
                        .collect(),
                )?,
                Ty::Range(int) => parts(vec![
                    (nth(values, 0), *int, ".start".into()),
                    (nth(values, 1), *int, ".end".into()),
                ])?,
                _ => return Err(("a kind of value that can't be kept yet", Vec::new())),
            },
            Value::Ref(_) | Value::Slice { .. } => return Err(memory()),
        })
    }
}

// --- Numbers ---

fn round(x: f64, float: FloatTy) -> f64 {
    match float {
        FloatTy::F32 => (x as f32) as f64,
        FloatTy::F64 => x,
    }
}

/// How many bits an `int` has, and whether it's signed. `usize` and `isize` are the host's.
fn width(int: IntTy) -> (u32, bool) {
    match int {
        IntTy::I8 => (8, true),
        IntTy::I16 => (16, true),
        IntTy::I32 => (32, true),
        IntTy::I64 => (64, true),
        IntTy::I128 => (128, true),
        IntTy::ISize => (usize::BITS, true),
        IntTy::U8 => (8, false),
        IntTy::U16 => (16, false),
        IntTy::U32 => (32, false),
        IntTy::U64 => (64, false),
        IntTy::U128 => (128, false),
        IntTy::USize => (usize::BITS, false),
    }
}

/// The smallest and largest `int`. A `u128` past `i128::MAX` can't be held.
fn bounds(int: IntTy) -> (i128, i128) {
    match width(int) {
        (128, _) => (i128::MIN, i128::MAX),
        (bits, true) => (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1),
        (bits, false) => (0, (1i128 << bits) - 1),
    }
}

/// `n` wrapped to `int`'s width.
fn wrap(n: i128, int: IntTy) -> i128 {
    let (bits, signed) = width(int);
    if bits == 128 {
        return n;
    }
    let modulus = 1i128 << bits;
    let min = if signed { -modulus / 2 } else { 0 };
    n.wrapping_sub(min).rem_euclid(modulus) + min
}

/// Whether `a / b` is `MIN / -1` in a signed type, which traps.
fn traps(a: i128, b: i128, int: IntTy) -> bool {
    let (bits, signed) = width(int);
    signed && b == -1 && a == bounds(int).0 && bits > 0
}

/// `a << n` or `a >> n`, as the optimizer folds them: moved its width or more, or a negative
/// amount, none of `a`'s bits are left.
fn shift(op: BinOp, a: i128, n: i128, int: IntTy) -> i128 {
    let bits = width(int).0 as i128;
    if !(0..bits).contains(&n) {
        return if op == BinOp::ShiftRight && a < 0 {
            -1
        } else {
            0
        };
    }
    let moved = match op {
        BinOp::ShiftLeft => ((a as u128) << n) as i128,
        _ => a >> n,
    };
    wrap(moved, int)
}

// --- Operands ---

fn rvalue_operands(rvalue: &Rvalue) -> Vec<&Operand> {
    rvalue.operands()
}

/// Every operand in `body`.
fn operands(body: &Body) -> Vec<&Operand> {
    let mut found = Vec::new();
    for block in body.blocks.iter() {
        for statement in &block.stmts {
            if let StatementKind::Assign(_, rvalue) = &statement.kind {
                found.extend(rvalue_operands(rvalue));
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
            found.push(condition);
        }
    }
    found
}

/// Every operand in `body`, to change.
fn operands_mut(body: &mut Body) -> Vec<&mut Operand> {
    let mut found = Vec::new();
    for block in body.blocks.iter_mut() {
        for statement in &mut block.stmts {
            if let StatementKind::Assign(_, rvalue) = &mut statement.kind {
                found.extend(match rvalue {
                    Rvalue::Use(op)
                    | Rvalue::Not(op)
                    | Rvalue::Some(op)
                    | Rvalue::Repeat { operand: op, .. }
                    | Rvalue::Cast { operand: op, .. } => vec![op],
                    Rvalue::Binary { lhs, rhs, .. }
                    | Rvalue::Range {
                        start: lhs,
                        end: rhs,
                        ..
                    }
                    | Rvalue::MakeSlice {
                        start: lhs,
                        len: rhs,
                        ..
                    } => vec![lhs, rhs],
                    Rvalue::Struct { fields: ops, .. }
                    | Rvalue::Array { items: ops, .. }
                    | Rvalue::Call { args: ops, .. }
                    | Rvalue::Intrinsic { args: ops, .. } => ops.iter_mut().collect(),
                    Rvalue::Ref { .. } => Vec::new(),
                });
            }
        }
        if let TerminatorKind::Branch { condition, .. } = &mut block.terminator.kind {
            found.push(condition);
        }
    }
    found
}

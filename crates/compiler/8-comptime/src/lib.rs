//! Works out each `@const` whose value runs code, by running its initializer while compiling,
//! then puts the value it gets in the program in place of every `ConstKind::Pending`.
//!
//! Runs after the borrow checker, so the code it runs has passed every check. It's a small
//! interpreter over MIR: each function call a frame of locals, each value a [`Value`]. It
//! does what the program would do when it runs: integers wrap to their type's width, `f32`
//! rounds to `f32` at every step, and `usize` is the host's width, like the C backend's.

mod error;

pub use error::{ComptimeError, STEP_LIMIT, Working};

use som_base::Span;
use som_mir::*;
use std::collections::HashMap;
use std::rc::Rc;

/// Works out every initializer the program uses, and puts their values in its place. If
/// one can't be worked out, says why, and leaves the program as it was.
pub fn run(mir: &mut Mir) -> Vec<ComptimeError> {
    let mut wanted = Vec::new();
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
        };
        let result = machine.run(init);
        let trace = machine.trace();
        self.running.pop();
        let result = match result {
            Ok(value) => match bake(mir, &value, mir.funcs[init].ret) {
                Ok(kind) => Some(kind),
                Err(what) => {
                    self.errors.push(ComptimeError::ConstHoldsMemory {
                        name: const_name(mir, init),
                        what,
                        span: mir.funcs[init].span.clone(),
                    });
                    None
                }
            },
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
#[derive(Clone, Debug)]
enum Value {
    /// A local before it's assigned, or after its scope ends.
    Uninit,
    Int(i128),
    Float(f64),
    Bool(bool),
    Unit,
    /// A text literal: its text, and the type it was written as.
    Str(Rc<str>),
    Func(FuncId),
    /// A struct's, enum's, array's, optional's or range's parts, in order.
    Agg(Vec<Value>),
    /// A reference: where it points.
    Ref(Loc),
    /// A null pointer.
    Null,
}

/// Where a value is: a frame's local, then a path through its parts.
#[derive(Clone, Debug)]
struct Loc {
    frame: usize,
    local: Local,
    path: Vec<usize>,
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
        }
    }
}

fn unsupported<T>(what: &'static str, span: &Span) -> Result<T, Stop> {
    Err(Stop::Report(Fault::NotSupported(what), span.clone()))
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
    /// Where what it returns goes, in the frame that called it.
    dest: Option<Loc>,
    /// Where it was called.
    call_span: Span,
}

/// Runs one initializer.
struct Machine<'s, 'm> {
    state: &'s mut State<'m>,
    frames: Vec<Frame>,
    steps: u64,
}

impl Machine<'_, '_> {
    fn mir(&self) -> &Mir {
        self.state.mir
    }

    fn body(&self, func: FuncId) -> &Body {
        self.mir().funcs[func]
            .body
            .as_ref()
            .expect("only functions with bodies are called")
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
        if let Some(first) = self.frames.first() {
            trace.push(Working {
                name: const_name(self.mir(), first.func),
                span: first.call_span.clone(),
                call: false,
            });
        }
        trace
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
                // `@unreachable`'s call, which says where it is, then stops.
                if let StatementKind::Assign(_, Rvalue::Call { func, .. }) = &statement.kind
                    && Some(*func) == mir.unreachable
                {
                    return Err(Stop::Report(Fault::Unreachable, statement.span.clone()));
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
                        Some(dest) => *self.slot(&dest, span)? = value,
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
        dest: Option<Loc>,
        span: Span,
    ) -> Result<(), Stop> {
        let function = &self.mir().funcs[func];
        let Some(body) = &function.body else {
            let name = function.link_name.clone().unwrap_or(function.name.clone());
            return Err(Stop::Report(Fault::CallsExtern(name), span));
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
                let dest = self.loc(place, span)?;
                *self.slot(&dest, span)? = value;
                Ok(())
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
                let at = self.loc(place, span)?;
                self.push_frame(*glue, vec![Value::Ref(at)], None, span.clone())
            }
        }
    }

    // --- Places ---

    /// Where `place` is, in the innermost frame.
    fn loc(&self, place: &Place, span: &Span) -> Result<Loc, Stop> {
        let frame = self.frames.len() - 1;
        let mut loc = Loc {
            frame,
            local: place.local,
            path: Vec::new(),
        };
        for projection in &place.projection {
            match projection {
                Projection::Field(i) => loc.path.push(*i),
                Projection::Index(local) => match self.frames[frame].locals[local.id] {
                    Value::Int(i) => loc.path.push(i as usize),
                    _ => return unsupported("an index that isn't a number", span),
                },
                Projection::Deref => match self.read(&loc, span)? {
                    Value::Ref(target) => loc = target,
                    Value::Null => return unsupported("a null pointer", span),
                    _ => return unsupported("slices and text", span),
                },
            }
        }
        Ok(loc)
    }

    fn read(&self, loc: &Loc, span: &Span) -> Result<Value, Stop> {
        let mut value = &self.frames[loc.frame].locals[loc.local.id];
        for &i in &loc.path {
            value = match value {
                Value::Agg(parts) => parts.get(i).unwrap_or(&Value::Uninit),
                Value::Uninit => &Value::Uninit,
                _ => return unsupported("slices and text", span),
            };
        }
        Ok(value.clone())
    }

    /// The value at `loc`, to change: the parts on the way are made if they aren't yet.
    fn slot(&mut self, loc: &Loc, span: &Span) -> Result<&mut Value, Stop> {
        let mut value = &mut self.frames[loc.frame].locals[loc.local.id];
        for &i in &loc.path {
            if matches!(value, Value::Uninit) {
                *value = Value::Agg(Vec::new());
            }
            value = match value {
                Value::Agg(parts) => {
                    if parts.len() <= i {
                        parts.resize(i + 1, Value::Uninit);
                    }
                    &mut parts[i]
                }
                _ => return unsupported("slices and text", span),
            };
        }
        Ok(value)
    }

    // --- Values ---

    fn operand(&mut self, op: &Operand, span: &Span) -> Result<Value, Stop> {
        match op {
            Operand::Copy(place) | Operand::Move(place) => {
                let loc = self.loc(place, span)?;
                self.read(&loc, span)
            }
            Operand::Const(c) => self.constant(c, span),
        }
    }

    fn constant(&mut self, c: &Const, span: &Span) -> Result<Value, Stop> {
        Ok(match &c.kind {
            ConstKind::Int(n) => Value::Int(*n),
            ConstKind::Float(x) => Value::Float(*x),
            ConstKind::Bool(b) => Value::Bool(*b),
            ConstKind::Str(s) => Value::Str(s.as_str().into()),
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

    /// A value of type `ty` with every byte zero.
    fn zero(&self, ty: TypeId, span: &Span) -> Result<Value, Stop> {
        let mir = self.mir();
        Ok(match mir.types.get(ty) {
            Ty::Unit => Value::Unit,
            Ty::Bool => Value::Bool(false),
            Ty::Char | Ty::Int(_) => Value::Int(0),
            Ty::Float(_) => Value::Float(0.0),
            Ty::Pointer | Ty::Ptr(_) | Ty::CStr | Ty::Ref { .. } => Value::Null,
            Ty::Struct(def, args) => {
                let s = mir.struct_def(*def, args);
                if s.handle {
                    Value::Null
                } else {
                    let fields: Vec<TypeId> = s.fields.iter().map(|&(_, ty)| ty).collect();
                    Value::Agg(
                        (fields.into_iter())
                            .map(|ty| self.zero(ty, span))
                            .collect::<Result<_, _>>()?,
                    )
                }
            }
            Ty::Optional(inner) => Value::Agg(vec![Value::Bool(false), self.zero(*inner, span)?]),
            Ty::Array { element, len } => {
                let len = self.array_len(*len, span)?;
                Value::Agg(vec![self.zero(*element, span)?; len])
            }
            Ty::Range(_) => Value::Agg(vec![Value::Int(0), Value::Int(0)]),
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
                let body = self.body(frame.func);
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
            Rvalue::Ref { place, .. } => Value::Ref(self.loc(place, span)?),
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
                ty,
                variant: None,
                fields,
            } => {
                let _ = ty;
                Value::Agg(operands(self, fields)?)
            }
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
                let indexes = self
                    .mir()
                    .struct_def(*def, args)
                    .variant_fields(*variant)
                    .to_vec();
                // Its tag, first, says which variant it is.
                parts[0] = Value::Int(*variant as i128);
                for (index, value) in indexes.into_iter().zip(operands(self, fields)?) {
                    parts[index] = value;
                }
                Value::Agg(parts)
            }
            Rvalue::MakeSlice { .. } => return unsupported("slices and text", span),
            Rvalue::Intrinsic { .. } => return unsupported("raw memory", span),
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
}

/// `value`, of type `ty`, as a constant in the program, or what it is that can't be one.
fn bake(mir: &Mir, value: &Value, ty: TypeId) -> Result<ConstKind, &'static str> {
    let part = |value: &Value, ty: TypeId| -> Result<Const, &'static str> {
        Ok(Const {
            kind: bake(mir, value, ty)?,
            ty,
        })
    };
    // What its parts are, and their types.
    let parts = |values: &[Value], types: &[TypeId]| -> Result<ConstKind, &'static str> {
        let mut parts = Vec::new();
        for (i, &ty) in types.iter().enumerate() {
            parts.push(part(values.get(i).unwrap_or(&Value::Uninit), ty)?);
        }
        Ok(ConstKind::Aggregate(parts))
    };
    Ok(match value {
        Value::Int(n) => ConstKind::Int(*n),
        Value::Float(x) if x.is_finite() => ConstKind::Float(*x),
        Value::Float(_) => return Err("infinite, or not a number"),
        Value::Bool(b) => ConstKind::Bool(*b),
        Value::Unit => ConstKind::Unit,
        Value::Str(s) => ConstKind::Str(s.to_string()),
        Value::Func(f) => ConstKind::Func(*f),
        // Never set: a field of a variant it isn't, or `none`'s value.
        Value::Uninit | Value::Null => ConstKind::Zeroed,
        Value::Agg(values) => match mir.types.get(ty) {
            Ty::Struct(def, args) if !mir.struct_def(*def, args).handle => {
                let types: Vec<TypeId> = (mir.struct_def(*def, args).fields.iter())
                    .map(|&(_, ty)| ty)
                    .collect();
                parts(values, &types)?
            }
            Ty::Optional(inner) => parts(values, &[mir.types.bool, *inner])?,
            Ty::Array { element, .. } => parts(values, &vec![*element; values.len()])?,
            Ty::Range(int) => parts(values, &[*int, *int])?,
            _ => return Err("a kind of value that can't be kept yet"),
        },
        Value::Ref(_) => return Err("a reference to memory that only exists while compiling"),
    })
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

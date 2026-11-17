use super::{Frame, Machine};
use crate::limits::DEPTH_LIMIT;
use crate::*;
use som_base::Span;
use som_lang::Builtin;

impl Machine<'_, '_> {
    /// How running got where it is, from the innermost call out: each call, then the
    /// constant being worked out.
    pub(crate) fn trace(&self) -> Vec<Working> {
        let mut trace: Vec<Working> = (self.frames.iter().skip(1).rev())
            .map(|frame| Working {
                name: self.mir().funcs[frame.func].name.clone(),
                span: frame.call_span.clone(),
                call: true,
            })
            .collect();
        // One with no name is the line it's on: nothing to add.
        if let Some(first) = self.frames.first()
            && self.mir().funcs[first.func].name != Builtin::Const.name()
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
    pub(crate) fn std_frames(&self) -> usize {
        (self.frames.iter().skip(1).rev())
            .take_while(|frame| is_std(&self.mir().funcs[frame.func].name))
            .count()
    }

    /// Where to say running stopped, given it stopped at `span`: there, or where the program
    /// called into the standard library.
    pub(crate) fn blame(&self, span: Span) -> Span {
        match self.std_frames() {
            0 => span,
            n => self.frames[self.frames.len() - n].call_span.clone(),
        }
    }

    pub(crate) fn run(&mut self, init: FuncId) -> Result<Value, Stop> {
        self.run_with(init, Vec::new())
    }

    pub(crate) fn run_with(&mut self, func: FuncId, args: Vec<Value>) -> Result<Value, Stop> {
        let span = self.mir().funcs[func].span.clone();
        self.push_frame(func, args, None, span)?;
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
            if self.steps > self.limit {
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

    pub(crate) fn jump(&mut self, target: BlockId) {
        let frame = self.frames.last_mut().expect("a frame");
        frame.block = target;
        frame.stmt = 0;
    }

    pub(crate) fn push_frame(
        &mut self,
        func: FuncId,
        args: Vec<Value>,
        dest: Option<(Loc, TypeId)>,
        span: Span,
    ) -> Result<(), Stop> {
        let function = &self.mir().funcs[func];
        let Some(body) = &function.body else {
            let name = c_name(function);
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

    pub(crate) fn statement(&mut self, statement: &Statement) -> Result<(), Stop> {
        let span = &statement.span;
        let top = self.frames.len() - 1;
        match &statement.kind {
            StatementKind::Assign(
                place,
                Rvalue::Call {
                    func,
                    args: operands,
                },
            ) => {
                let args = (operands.iter())
                    .map(|a| self.operand(a, span))
                    .collect::<Result<Vec<_>, _>>()?;
                let dest = self.loc(place, span)?;
                let function = &self.mir().funcs[*func];
                if function.body.is_none() {
                    // One of C's done here; any other, called for real.
                    let name = c_name(function);
                    let value = match self.native(&name, &args, span)? {
                        Some(value) => value,
                        None => {
                            let tys: Vec<TypeId> =
                                operands.iter().map(|a| self.operand_ty(a)).collect();
                            self.call_foreign(*func, &args, &tys, span)?
                        }
                    };
                    let (dest, ty) = dest;
                    return self.write(&dest, ty, value, span);
                }
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
}

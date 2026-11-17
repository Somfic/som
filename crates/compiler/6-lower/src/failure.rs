use som_ast::Span;
use som_lang::{Builtin, FailureField, LangFunc, LangStruct, LocationField};
use som_mir::*;
use som_resolve::{DefId, DefKind};

use super::instances::internal_function;
use super::{FnBuilder, Lowerer};

const FAILED: &str = "som::failed";

impl Lowerer<'_> {
    /// What `@fail` and `@unreachable` call with a `Failure`: each of the program's `@on fail`
    /// functions, in order, then the standard library's `halt`.
    pub(super) fn failed(&mut self) -> Option<FuncId> {
        if let Some(failed) = self.failed {
            return failed;
        }
        let failed = self.make_failed();
        self.failed = Some(failed);
        failed
    }

    fn make_failed(&mut self) -> Option<FuncId> {
        let def = self.names.lang_struct(LangStruct::Failure)?;
        let DefKind::Func(halt) = self.names.def(self.names.lang_func(LangFunc::Halt)?).kind else {
            return None;
        };
        let failure = self.mir.types.intern(Ty::Struct(def, Vec::new()));
        let by_ref = self.mir.types.intern(Ty::Ref {
            mutable: false,
            to: failure,
        });
        let unit = self.mir.types.unit;
        let span = self.names.def(def).span.clone();
        let function = internal_function(FAILED.into(), vec![failure], unit, span.clone());
        let id = self.mir.funcs.alloc(function);
        self.mir.fail = Some(id);
        let listeners: Vec<FuncId> = (self.names.fail_listeners().to_vec().into_iter())
            .map(|listener| self.instance(listener, Vec::new()))
            .collect();
        let halt = self.instance(halt, Vec::new());
        let body = failed_body([unit, failure, by_ref], &listeners, halt, span);
        self.mir.funcs[id].body = Some(body);
        Some(id)
    }
}

/// `listener(&failure)` for each listener, then `halt()`.
fn failed_body(
    [unit, failure, by_ref]: [TypeId; 3],
    listeners: &[FuncId],
    halt: FuncId,
    span: Span,
) -> Body {
    let mut locals = Arena::new();
    let local = |locals: &mut Arena<LocalDecl>, ty, name: Option<&str>| {
        locals.alloc(LocalDecl {
            ty,
            name: name.map(str::to_string),
            mutable: false,
            span: span.clone(),
        })
    };
    local(&mut locals, unit, None);
    let param = local(&mut locals, failure, Some("failure"));
    let statement = |kind| Statement {
        kind,
        span: span.clone(),
        reads: Vec::new(),
    };
    let mut stmts = Vec::new();
    for &listener in listeners {
        let reference = local(&mut locals, by_ref, None);
        stmts.push(statement(StatementKind::Assign(
            Place::local(reference),
            Rvalue::Ref {
                mutable: false,
                place: Place::local(param),
            },
        )));
        let called = local(&mut locals, unit, None);
        stmts.push(statement(StatementKind::Assign(
            Place::local(called),
            Rvalue::Call {
                func: listener,
                args: vec![Operand::Copy(Place::local(reference))],
            },
        )));
    }
    let halted = local(&mut locals, unit, None);
    stmts.push(statement(StatementKind::Assign(
        Place::local(halted),
        Rvalue::Call {
            func: halt,
            args: Vec::new(),
        },
    )));
    let mut blocks = Arena::new();
    blocks.alloc(Block {
        stmts,
        terminator: Terminator {
            kind: TerminatorKind::Unreachable,
            span: span.clone(),
        },
    });
    Body {
        locals,
        arg_count: 1,
        outs: Vec::new(),
        two_phase: Vec::new(),
        blocks,
    }
}

fn constant(kind: ConstKind, ty: TypeId) -> Operand {
    Operand::Const(Const { kind, ty })
}

impl FnBuilder<'_, '_> {
    /// Tells the program's `@on fail` functions what failed, `message` (`None` for
    /// `@unreachable`), and where, then stops the program.
    pub(super) fn fail(&mut self, message: Option<Operand>, span: Span) {
        if let Some(failed) = self.l.failed()
            && let Some(location) = self.l.names.lang_struct(LangStruct::Location)
            && let Some(failure) = self.l.names.lang_struct(LangStruct::Failure)
        {
            let at = self.location(location, &span);
            let value = self.failure(failure, message, at, &span);
            let unit = self.l.mir.types.unit;
            let called = self.temp(unit, span.clone());
            let call = Rvalue::Call {
                func: failed,
                args: vec![Operand::Move(Place::local(value))],
            };
            self.assign(Place::local(called), call, span.clone());
        }
        self.terminate(TerminatorKind::Unreachable, span);
        self.current = self.new_block();
    }

    fn location(&mut self, location: DefId, span: &Span) -> Local {
        let file = std::path::Path::new(span.source.identifier())
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let function = match self.func {
            Some(func) => self.l.ast.funcs[func].name.name.to_string(),
            None => Builtin::Const.name().to_string(),
        };
        self.lang_struct_value(location, span, |name, ty| {
            let kind = match LocationField::from_name(name) {
                Some(LocationField::File) => ConstKind::Str(file.clone()),
                Some(LocationField::Line) => ConstKind::Int(span.start.line as i128),
                Some(LocationField::Column) => ConstKind::Int(span.start.col as i128),
                Some(LocationField::Function) => ConstKind::Str(function.clone()),
                None => ConstKind::Zeroed,
            };
            constant(kind, ty)
        })
    }

    fn failure(
        &mut self,
        failure: DefId,
        message: Option<Operand>,
        at: Local,
        span: &Span,
    ) -> Local {
        let unreachable = message.is_none();
        let mut message = message;
        self.lang_struct_value(failure, span, |name, ty| {
            match FailureField::from_name(name) {
                Some(FailureField::Message) => message.take().unwrap_or_else(|| {
                    let reached = format!("reached {}", Builtin::Unreachable.name());
                    constant(ConstKind::Str(reached), ty)
                }),
                Some(FailureField::At) => Operand::Move(Place::local(at)),
                Some(FailureField::Unreachable) => constant(ConstKind::Bool(unreachable), ty),
                None => constant(ConstKind::Zeroed, ty),
            }
        })
    }

    /// A temporary holding a value of the standard library's non-generic struct `def`, with
    /// `field` giving each field from its name and type.
    fn lang_struct_value(
        &mut self,
        def: DefId,
        span: &Span,
        mut field: impl FnMut(&str, TypeId) -> Operand,
    ) -> Local {
        let ty = self.l.mir.types.intern(Ty::Struct(def, Vec::new()));
        let fields = (self.l.mir.struct_def(def, &[]).fields.iter())
            .map(|(name, ty)| field(name, *ty))
            .collect();
        let value = self.temp(ty, span.clone());
        let built = Rvalue::Struct {
            ty,
            variant: None,
            fields,
        };
        self.assign(Place::local(value), built, span.clone());
        value
    }
}

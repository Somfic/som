use som_ast::Span;
use som_lang::{FailureField, LangFunc, LangStruct, LocationField};
use som_mir::*;
use som_resolve::DefKind;

use super::{FnBuilder, Lowerer};

impl Lowerer<'_> {
    /// The function `@fail` and `@unreachable` call with a `Failure`: it calls each of the
    /// program's `@on fail` functions with it, in order, then the standard library's `halt`.
    pub(super) fn failed(&mut self) -> Option<FuncId> {
        if let Some(failed) = self.failed {
            return failed;
        }
        let failed = self.make_failed();
        self.failed = Some(failed);
        failed
    }

    pub(super) fn make_failed(&mut self) -> Option<FuncId> {
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
        let id = self.mir.funcs.alloc(Function {
            name: "som::failed".into(),
            params: vec![failure],
            ret: unit,
            span: span.clone(),
            body: None,
            link_name: None,
            library: None,
            is_entry: false,
            variadic: false,
        });
        self.mir.fail = Some(id);
        let listeners: Vec<FuncId> = (self.names.fail_listeners().to_vec().into_iter())
            .map(|listener| self.instance(listener, Vec::new()))
            .collect();
        let halt = self.instance(halt, Vec::new());

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
        for listener in listeners {
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
        self.mir.funcs[id].body = Some(Body {
            locals,
            arg_count: 1,
            outs: Vec::new(),
            two_phase: Vec::new(),
            blocks,
        });
        Some(id)
    }
}

impl FnBuilder<'_, '_> {
    /// Stops here, failing: tells the program's `@on fail` functions what failed, `message`
    /// (`None` for `@unreachable`), and where, then stops the program.
    pub(super) fn fail(&mut self, message: Option<Operand>, span: Span) {
        let ast = self.l.ast;
        if let Some(failed) = self.l.failed()
            && let Some(location) = self.l.names.lang_struct(LangStruct::Location)
            && let Some(failure) = self.l.names.lang_struct(LangStruct::Failure)
        {
            let file = std::path::Path::new(span.source.identifier())
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let function = match self.func {
                Some(func) => ast.funcs[func].name.name.to_string(),
                None => "@const".to_string(),
            };
            let location_ty = self.l.mir.types.intern(Ty::Struct(location, Vec::new()));
            let fields: Vec<(String, TypeId)> = self.l.mir.struct_def(location, &[]).fields.clone();
            let parts = (fields.iter())
                .map(|(name, ty)| match LocationField::from_name(name) {
                    Some(LocationField::File) => self.constant(ConstKind::Str(file.clone()), *ty),
                    Some(LocationField::Line) => {
                        self.constant(ConstKind::Int(span.start.line as i128), *ty)
                    }
                    Some(LocationField::Column) => {
                        self.constant(ConstKind::Int(span.start.col as i128), *ty)
                    }
                    Some(LocationField::Function) => {
                        self.constant(ConstKind::Str(function.clone()), *ty)
                    }
                    None => self.constant(ConstKind::Zeroed, *ty),
                })
                .collect();
            let at = self.temp(location_ty, span.clone());
            let value = Rvalue::Struct {
                ty: location_ty,
                variant: None,
                fields: parts,
            };
            self.assign(Place::local(at), value, span.clone());
            let failure_ty = self.l.mir.types.intern(Ty::Struct(failure, Vec::new()));
            let fields: Vec<(String, TypeId)> = self.l.mir.struct_def(failure, &[]).fields.clone();
            let unreachable = message.is_none();
            let mut message = message;
            let parts = (fields.iter())
                .map(|(name, ty)| match FailureField::from_name(name) {
                    Some(FailureField::Message) => message.take().unwrap_or_else(|| {
                        self.constant(ConstKind::Str("reached @unreachable".into()), *ty)
                    }),
                    Some(FailureField::At) => Operand::Move(Place::local(at)),
                    Some(FailureField::Unreachable) => {
                        self.constant(ConstKind::Bool(unreachable), *ty)
                    }
                    None => self.constant(ConstKind::Zeroed, *ty),
                })
                .collect();
            let value = self.temp(failure_ty, span.clone());
            let built = Rvalue::Struct {
                ty: failure_ty,
                variant: None,
                fields: parts,
            };
            self.assign(Place::local(value), built, span.clone());
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
}

//! Where a standard-library function that can fail was called from: `assert(x == 4)` fails
//! at the `assert` in the program, not at the `@fail` inside it.
//!
//! A standard-library function whose body fails (calls `som::failed`, at `@fail` or
//! `@unreachable`), or calls one that does, takes a hidden last parameter, the caller's
//! `Location`, and reports its failures there. Every call to one passes where the call is,
//! or from inside another such function, its own caller's. Like Rust's `#[track_caller]`.

use std::collections::{HashMap, HashSet};

use som_lang::{FailureField, LangStruct, LocationField};
use som_mir::*;

use super::Lowerer;

/// The hidden parameter's name, for the MIR's readers.
const CALLER: &str = "caller";

impl Lowerer<'_> {
    pub(super) fn pass_caller_locations(&mut self) {
        let Some(fail) = self.mir.fail else {
            return;
        };
        let (Some(location), Some(failure)) = (
            self.names.lang_struct(LangStruct::Location),
            self.names.lang_struct(LangStruct::Failure),
        ) else {
            return;
        };
        let location_ty = self.mir.types.intern(Ty::Struct(location, Vec::new()));
        let failure_ty = self.mir.types.intern(Ty::Struct(failure, Vec::new()));
        // Passed on as it is, from one such function to the next.
        if !self.is_copy(location_ty) {
            return;
        }
        let at = (self.mir.struct_def(failure, &[]).fields.iter())
            .position(|(name, _)| FailureField::from_name(name) == Some(FailureField::At));
        let fields: Vec<(String, TypeId)> = (self.mir.struct_def(location, &[]).fields.iter())
            .map(|(name, ty)| (name.to_string(), *ty))
            .collect();

        let origin: HashMap<FuncId, som_ast::Id<som_ast::Func>> = (self.instances.iter())
            .chain(&self.other_instances)
            .map(|((func, _), &id)| (id, *func))
            .collect();
        let tracked = self.tracked(fail, &origin);

        // Each one's parameter, right after its own.
        let mut caller_of = HashMap::new();
        for &id in &tracked {
            let function = &mut self.mir.funcs[id];
            let Some(body) = &mut function.body else {
                continue;
            };
            function.params.push(location_ty);
            let span = function.span.clone();
            let added = body.locals.alloc(LocalDecl {
                ty: location_ty,
                name: Some(CALLER.to_string()),
                mutable: false,
                span,
            });
            let param = Id::new(body.arg_count + 1);
            body.swap_locals(added, param);
            body.arg_count += 1;
            caller_of.insert(id, param);
            // Its failures are the caller's.
            for block in body.blocks.iter_mut() {
                for statement in &mut block.stmts {
                    if let StatementKind::Assign(_, Rvalue::Struct { ty, fields, .. }) =
                        &mut statement.kind
                        && *ty == failure_ty
                        && let Some(at) = at
                    {
                        fields[at] = Operand::Copy(Place::local(param));
                    }
                }
            }
        }

        let ids: Vec<FuncId> = self.mir.funcs.iter_with_ids().map(|(id, _)| id).collect();
        for id in ids {
            let function_name = match origin.get(&id) {
                Some(&func) => self.ast.funcs[func].name.name.to_string(),
                None => self.mir.funcs[id].name.clone(),
            };
            let caller = caller_of.get(&id).copied();
            let Some(body) = &mut self.mir.funcs[id].body else {
                continue;
            };
            for block in body.blocks.iter_mut() {
                for statement in &mut block.stmts {
                    let StatementKind::Assign(_, Rvalue::Call { func, args }) = &mut statement.kind
                    else {
                        continue;
                    };
                    if !tracked.contains(func) {
                        continue;
                    }
                    args.push(match caller {
                        Some(param) => Operand::Copy(Place::local(param)),
                        None => Operand::Const(Const {
                            kind: ConstKind::Aggregate(location_parts(
                                &fields,
                                &statement.span,
                                &function_name,
                            )),
                            ty: location_ty,
                        }),
                    });
                }
            }
        }
    }

    /// The standard library's functions that fail, or call one that does: each with a body,
    /// and none taken as a value, where it couldn't be passed a location.
    fn tracked(
        &self,
        fail: FuncId,
        origin: &HashMap<FuncId, som_ast::Id<som_ast::Func>>,
    ) -> HashSet<FuncId> {
        // A test is where failures are said to be, as the program's own code is: the
        // standard library's tests too.
        let in_std = |id: &FuncId| {
            origin.get(id).is_some_and(|&func| {
                let module = self.names.def(self.names.func(func)).module;
                som_lang::is_std_module(&self.ast.modules[module].name)
                    && !self.ast.funcs[func].test
            })
        };
        let callees = |body: &Body| -> Vec<FuncId> {
            (body.blocks.iter())
                .flat_map(|block| &block.stmts)
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign(_, Rvalue::Call { func, .. }) => Some(*func),
                    _ => None,
                })
                .collect()
        };
        let calls: Vec<(FuncId, Vec<FuncId>)> = (self.mir.funcs.iter_with_ids())
            .filter(|(id, _)| in_std(id))
            .filter_map(|(id, f)| Some((id, callees(f.body.as_ref()?))))
            .collect();
        let mut tracked: HashSet<FuncId> = HashSet::new();
        loop {
            let more: Vec<FuncId> = (calls.iter())
                .filter(|(id, callees)| {
                    !tracked.contains(id)
                        && callees.iter().any(|c| *c == fail || tracked.contains(c))
                })
                .map(|(id, _)| *id)
                .collect();
            if more.is_empty() {
                break;
            }
            tracked.extend(more);
        }
        // Taken as a value: called without the program knowing it's one of these.
        for function in self.mir.funcs.iter() {
            let Some(body) = &function.body else {
                continue;
            };
            for operand in body.operands() {
                if let Operand::Const(Const {
                    kind: ConstKind::Func(id),
                    ..
                }) = operand
                {
                    tracked.remove(id);
                }
            }
        }
        tracked
    }
}

/// `Location { file, line, column, function }` of a call at `span`, in `function`.
fn location_parts(fields: &[(String, TypeId)], span: &som_ast::Span, function: &str) -> Vec<Const> {
    let file = std::path::Path::new(span.source.identifier())
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    (fields.iter())
        .map(|(field, ty)| Const {
            kind: match LocationField::from_name(field) {
                Some(LocationField::File) => ConstKind::Str(file.clone()),
                Some(LocationField::Line) => ConstKind::Int(span.start.line as i128),
                Some(LocationField::Column) => ConstKind::Int(span.start.col as i128),
                Some(LocationField::Function) => ConstKind::Str(function.to_string()),
                None => ConstKind::Zeroed,
            },
            ty: *ty,
        })
        .collect()
}

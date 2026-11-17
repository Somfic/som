//! `som::run_tests`, what `som test` starts at: written here rather than in the standard
//! library, since a test needn't be visible from there. Running one is the library's
//! (`std/testing.som`):
//!
//! ```text
//! failed = 0
//! if test_start(&mut failed, "parses_hex", Location { … }) { parses_hex(); test_passed(); }
//! …
//! return test_summary(failed, count)
//! ```

use som_ast::Span;
use som_lang::{LangFunc, LangStruct, LocationField};
use som_mir::*;
use som_resolve::DefKind;

use super::Lowerer;
use super::instances::internal_function;

const RUN_TESTS: &str = "som::run_tests";

impl Lowerer<'_> {
    /// Builds `som::run_tests` into `mir.tests`, if the standard library has a runner.
    pub(super) fn make_test_runner(&mut self) {
        let Some([start, passed, summary]) = self.test_runner_funcs() else {
            return;
        };
        let Some(location) = self.names.lang_struct(LangStruct::Location) else {
            return;
        };
        let ([failed_ref, name_ty, location_ty], yes_no) = {
            let f = &self.mir.funcs[start];
            let [a, b, c] = f.params[..] else {
                return;
            };
            ([a, b, c], f.ret)
        };
        let (i32_ty, ret) = {
            let f = &self.mir.funcs[summary];
            (f.params[0], f.ret)
        };
        let fields: Vec<(String, TypeId)> = (self.mir.struct_def(location, &[]).fields.iter())
            .map(|(name, ty)| (name.to_string(), *ty))
            .collect();

        // The program's own. The standard library's only when it's the program: then the
        // root module is an empty one in its folder.
        let modules = &self.ast.modules;
        let std_is_program = (modules.iter())
            .find(|m| *m.name == *som_lang::PRELUDE_MODULE)
            .is_some_and(|std| std.path == modules[0].path);
        let tests: Vec<(FuncId, String, Span)> = (self.names.tests().to_vec().into_iter())
            .filter(|&test| {
                let module = &modules[self.names.def(self.names.func(test)).module];
                std_is_program || !som_lang::is_std_module(&module.name)
            })
            .map(|test| {
                let func = &self.ast.funcs[test];
                let module = &self.ast.modules[self.names.def(self.names.func(test)).module].name;
                let name = match module.is_empty() {
                    true => func.name.name.to_string(),
                    false => format!("{module}::{}", func.name.name),
                };
                (
                    self.instance(test, Vec::new()),
                    name,
                    func.name.span.clone(),
                )
            })
            .collect();

        let span = self.names.def(location).span.clone();
        let mut b = Body {
            locals: Arena::new(),
            arg_count: 0,
            outs: Vec::new(),
            two_phase: Vec::new(),
            blocks: Arena::new(),
        };
        let local = |b: &mut Body, ty, name: Option<&str>, mutable| {
            b.locals.alloc(LocalDecl {
                ty,
                name: name.map(str::to_string),
                mutable,
                span: span.clone(),
            })
        };
        local(&mut b, ret, None, false);
        let failed = local(&mut b, i32_ty, Some("failed"), true);
        let statement = |kind, span: &Span| Statement {
            kind,
            span: span.clone(),
            reads: Vec::new(),
        };
        let call = |func, args| Rvalue::Call { func, args };
        let unit = self.mir.types.unit;

        let mut stmts = vec![statement(
            StatementKind::Assign(
                Place::local(failed),
                Rvalue::Use(constant(ConstKind::Int(0), i32_ty)),
            ),
            &span,
        )];
        // Each test's block, then the child's; the test's branches to the next test's once
        // that's made.
        let mut branches = Vec::new();
        for (test, name, at) in &tests {
            let reference = local(&mut b, failed_ref, None, false);
            stmts.push(statement(
                StatementKind::Assign(
                    Place::local(reference),
                    Rvalue::Ref {
                        mutable: true,
                        place: Place::local(failed),
                    },
                ),
                at,
            ));
            let place = local(&mut b, location_ty, None, false);
            stmts.push(statement(
                StatementKind::Assign(
                    Place::local(place),
                    location_value(location_ty, &fields, name, at),
                ),
                at,
            ));
            let child = local(&mut b, yes_no, None, false);
            let args = vec![
                Operand::Move(Place::local(reference)),
                constant(ConstKind::Str(name.clone()), name_ty),
                Operand::Move(Place::local(place)),
            ];
            stmts.push(statement(
                StatementKind::Assign(Place::local(child), call(start, args)),
                at,
            ));
            let here = b.blocks.alloc(Block {
                stmts: std::mem::take(&mut stmts),
                terminator: Terminator {
                    kind: TerminatorKind::Unreachable,
                    span: at.clone(),
                },
            });
            // The child runs the test, and leaves in `test_passed` if it returns.
            let ran = local(&mut b, unit, None, false);
            let left = local(&mut b, unit, None, false);
            let in_child = b.blocks.alloc(Block {
                stmts: vec![
                    statement(
                        StatementKind::Assign(Place::local(ran), call(*test, Vec::new())),
                        at,
                    ),
                    statement(
                        StatementKind::Assign(Place::local(left), call(passed, Vec::new())),
                        at,
                    ),
                ],
                terminator: Terminator {
                    kind: TerminatorKind::Unreachable,
                    span: at.clone(),
                },
            });
            branches.push((here, child, in_child));
        }
        let count = constant(ConstKind::Int(tests.len() as i128), i32_ty);
        let args = vec![Operand::Copy(Place::local(failed)), count];
        stmts.push(statement(
            StatementKind::Assign(Place::local(RETURN), call(summary, args)),
            &span,
        ));
        let last = b.blocks.alloc(Block {
            stmts,
            terminator: Terminator {
                kind: TerminatorKind::Return,
                span: span.clone(),
            },
        });
        let nexts: Vec<BlockId> = (branches.iter().skip(1).map(|&(here, ..)| here))
            .chain([last])
            .collect();
        for (&(here, child, in_child), next) in branches.iter().zip(nexts) {
            b.blocks[here].terminator.kind = TerminatorKind::Branch {
                condition: Operand::Copy(Place::local(child)),
                then: in_child,
                otherwise: next,
            };
        }

        let mut function = internal_function(RUN_TESTS.into(), Vec::new(), ret, span);
        function.body = Some(b);
        self.mir.tests = Some(self.mir.funcs.alloc(function));
    }

    /// `test_start`, `test_passed` and `test_summary`, from the standard library.
    fn test_runner_funcs(&mut self) -> Option<[FuncId; 3]> {
        let mut found = [
            LangFunc::TestStart,
            LangFunc::TestPassed,
            LangFunc::TestSummary,
        ]
        .map(|lang| {
            let def = self.names.lang_func(lang)?;
            match self.names.def(def).kind {
                DefKind::Func(func) => Some(func),
                _ => None,
            }
        })
        .into_iter();
        let [Some(start), Some(passed), Some(summary)] =
            [found.next()?, found.next()?, found.next()?]
        else {
            return None;
        };
        Some([start, passed, summary].map(|func| self.instance(func, Vec::new())))
    }
}

/// `Location { file, line, column, function }` for the test `name` at `at`.
fn location_value(ty: TypeId, fields: &[(String, TypeId)], name: &str, at: &Span) -> Rvalue {
    let file = std::path::Path::new(at.source.identifier())
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let fields = (fields.iter())
        .map(|(field, ty)| {
            let kind = match LocationField::from_name(field) {
                Some(LocationField::File) => ConstKind::Str(file.clone()),
                Some(LocationField::Line) => ConstKind::Int(at.start.line as i128),
                Some(LocationField::Column) => ConstKind::Int(at.start.col as i128),
                Some(LocationField::Function) => ConstKind::Str(name.to_string()),
                None => ConstKind::Zeroed,
            };
            constant(kind, *ty)
        })
        .collect();
    Rvalue::Struct {
        ty,
        variant: None,
        fields,
    }
}

fn constant(kind: ConstKind, ty: TypeId) -> Operand {
    Operand::Const(Const { kind, ty })
}

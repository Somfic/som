//! The passes after loading, in order, shared by building (which stops at the first pass
//! with errors) and editor tooling (which carries on as far as it can).

use som_ast::Ast;
use som_borrowcheck::BorrowError;
use som_mir::Mir;
use som_resolve::Names;
use som_typeck::{TypeError, Typed};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::imports::suggest_imports;
use crate::{Error, Options, Warning, headers, libraries, targets};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OnError {
    Stop,
    Continue,
}

/// The pipeline stopped at errors, under [`OnError::Stop`].
pub(crate) struct Stopped;

pub(crate) struct Passes<'a> {
    options: &'a Options,
    on_error: OnError,
    pub errors: Vec<Error>,
    pub warnings: Vec<Warning>,
}

/// A program with its names resolved and its types checked.
pub(crate) struct Front {
    pub ast: Ast,
    pub names: Names,
    pub typed: Typed,
    /// The library files its `extern "…"` blocks link.
    pub libraries: Vec<PathBuf>,
}

impl<'a> Passes<'a> {
    pub fn new(options: &'a Options, on_error: OnError) -> Self {
        Passes {
            options,
            on_error,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn report(&mut self, errors: impl IntoIterator<Item = Error>) -> Result<(), Stopped> {
        self.errors.extend(errors);
        match self.on_error {
            OnError::Stop if !self.errors.is_empty() => Err(Stopped),
            _ => Ok(()),
        }
    }

    /// Checks libraries, resolves names and checks types.
    pub fn front(&mut self, mut ast: Ast) -> Result<Front, Stopped> {
        let toolchain = &self.options.toolchain;
        // Missing functions and mismatched ones are reported together.
        let (libraries, library_errors) = libraries::check(&mut ast, toolchain);
        let (header_errors, header_warnings) = headers::check_declarations(&ast, toolchain);
        self.warnings
            .extend(header_warnings.into_iter().map(Warning::Library));
        self.report(
            (library_errors.into_iter())
                .chain(header_errors)
                .map(Error::Library),
        )?;

        let checking = match self.options.every_system {
            true => targets::Checking::EverySystem,
            false => targets::Checking::Target(toolchain.target.os()),
        };
        // Whatever's found is about another system, but for a name no system has.
        let left_out = targets::check_left_out(&ast, checking);
        self.warnings
            .extend(left_out.into_iter().map(Warning::Unavailable));
        let (names, mut resolve_errors) = som_resolve::resolve(&ast);
        suggest_imports(&ast, &mut resolve_errors, self.options);
        let inferred = targets::infer(&ast, &names);
        let cant_run = targets::check_entries(&ast, &names, &inferred, checking);
        if self.options.every_system {
            self.warnings
                .extend(cant_run.into_iter().map(Warning::CantRun));
            self.report(resolve_errors.into_iter().map(Error::Resolve))?;
        } else {
            self.report(
                (resolve_errors.into_iter().map(Error::Resolve))
                    .chain(cant_run.into_iter().map(Error::CantRun)),
            )?;
        }

        let (typed, type_errors) = typecheck_rounds(&ast, &names);
        self.report(type_errors)?;

        Ok(Front {
            ast,
            names,
            typed,
            libraries,
        })
    }

    /// Proves, lowers, borrow-checks and works out what's run while compiling. Only a
    /// program with no errors so far is lowered: `None` for one with them.
    pub fn back(&mut self, front: &Front) -> Result<Option<Mir>, Stopped> {
        let Front {
            ast, names, typed, ..
        } = front;
        if !self.errors.is_empty() {
            return Ok(None);
        }
        self.report(
            som_prove::prove(ast, names, typed)
                .into_iter()
                .map(Error::Prove),
        )?;
        if !self.errors.is_empty() {
            return Ok(None);
        }

        let mut mir = match lower_instances(ast, names, typed) {
            Ok(mir) => mir,
            Err(instance_errors) => {
                self.report(instance_errors)?;
                som_lower::lower(ast, names, typed)
            }
        };
        self.report((borrow_errors(ast, names, typed, &mir).into_iter()).map(Error::Borrow))?;
        // What's worked out while compiling, `@assert(…)` among it: its errors are the
        // program's too.
        if self.errors.is_empty() {
            self.report(som_comptime::run(&mut mir).into_iter().map(Error::Comptime))?;
        }
        if self.errors.is_empty() {
            self.warnings.extend(
                (som_comptime::could_be_const(&mir).into_iter()).map(Warning::CouldBeConst),
            );
        }
        Ok(Some(mir))
    }
}

/// Checks types, working out first the constants needed to: `[i32; SIZE]` of
/// `@const SIZE = next_power_of_two(100);` needs `SIZE` run before the type is known. Each
/// round checks the program with what's worked out so far; the constants it needed are then
/// lowered (only what they run), borrow-checked and run, and it's checked again. A round
/// that works nothing new out is a constant needed to work itself out.
fn typecheck_rounds(ast: &Ast, names: &Names) -> (Typed, Vec<Error>) {
    let mut known = som_typeck::ConstValues::default();
    loop {
        let checked = som_typeck::check_with(ast, names, &known);
        if checked.needed.is_empty() || !checked.errors.is_empty() {
            let errors = checked.errors.into_iter().map(Error::Type).collect();
            return (checked.typed, errors);
        }
        let mut progress = false;
        for &c in &checked.needed {
            match work_out(ast, names, &checked.typed, c) {
                Ok(Some(value)) => {
                    known.0.insert(c, value);
                    progress = true;
                }
                // It needs another constant that isn't worked out yet.
                Ok(None) => {}
                Err(errors) => return (checked.typed, errors),
            }
        }
        if !progress {
            let errors = (checked.needed.iter())
                .map(|&c| {
                    let name = &ast.consts[c].name;
                    Error::Type(TypeError::ConstCycle {
                        name: name.name.to_string(),
                        span: name.span.clone(),
                    })
                })
                .collect();
            return (checked.typed, errors);
        }
    }
}

/// Constant `c`'s value, worked out by running it, with the program checked so far.
/// `Ok(None)` if what it runs needs a constant that isn't known yet: its types aren't all
/// known.
fn work_out(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    c: som_ast::Id<som_ast::Const>,
) -> Result<Option<som_typeck::ConstValue>, Vec<Error>> {
    let (mir, inits) = som_lower::lower_inits(ast, names, typed, &[c]);
    if mentions_unknown(&mir) {
        return Ok(None);
    }
    let errors = som_borrowcheck::check(&mir);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Borrow).collect());
    }
    let (values, errors) = som_comptime::evaluate(&mir, &inits);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Comptime).collect());
    }
    Ok(Some(match values.into_iter().next().flatten() {
        Some(som_mir::ConstKind::Int(n)) => som_typeck::ConstValue::Int(n),
        Some(som_mir::ConstKind::Bool(b)) => som_typeck::ConstValue::Bool(b),
        _ => som_typeck::ConstValue::Other,
    }))
}

/// How far into a type's parts [`mentions_unknown`] looks: a bound, so a type made of
/// itself can't recurse forever.
const MAX_TYPE_DEPTH: usize = 32;

/// Whether a function in `mir` has a type that isn't known: made of a constant still to be
/// worked out.
fn mentions_unknown(mir: &Mir) -> bool {
    fn unknown(mir: &Mir, ty: som_mir::TypeId, depth: usize) -> bool {
        depth < MAX_TYPE_DEPTH
            && match mir.types.get(ty) {
                som_mir::Ty::Error => true,
                t => (t.children().into_iter()).any(|child| unknown(mir, child, depth + 1)),
            }
    }
    mir.funcs.iter().any(|func| {
        unknown(mir, func.ret, 0)
            || func.params.iter().any(|&p| unknown(mir, p, 0))
            || (func.body.iter())
                .flat_map(|body| body.locals.iter())
                .any(|local| unknown(mir, local.ty, 0))
    })
}

/// Lowers the program, checking each instance of a function checked for each instance as
/// it's reached: its errors, each saying which instance, if any.
fn lower_instances(ast: &Ast, names: &Names, typed: &Typed) -> Result<Mir, Vec<Error>> {
    let mut seen = HashSet::new();
    let mut errors = Vec::new();
    let mut say = |name: &str, used_at: &som_base::Span, inner: Error| {
        // The same error in another instance, said once. By its text: the passes' errors
        // can't be compared.
        if seen.insert(format!("{inner:?}")) {
            errors.push(Error::InInstance {
                instance: name.to_string(),
                used_at: used_at.clone(),
                inner: Box::new(inner),
            });
        }
    };
    match som_lower::lower_checked(ast, names, typed) {
        Err(instances) => {
            for instance in instances {
                for error in instance.errors {
                    say(&instance.name, &instance.used_at, Error::Type(error));
                }
            }
        }
        Ok((mir, instances)) => {
            // Each instance proven, with its parameters known: what's in its `@if`s.
            let regions = typed.region_spans(ast);
            for instance in &instances {
                let proven = som_prove::prove_instance(
                    ast,
                    names,
                    &instance.typed,
                    instance.func,
                    &instance.values,
                    &regions,
                );
                for error in proven {
                    say(&instance.name, &instance.used_at, Error::Prove(error));
                }
            }
            if errors.is_empty() {
                return Ok(mir);
            }
        }
    }
    Err(errors)
}

/// The borrow checker's errors: in `mir`, the program as it's used, and in every generic
/// function with stand-ins for its type parameters, used or not, so a value used after it's
/// moved in one is found where it's written. Each once: a function the program uses with a
/// type that moves gives the same error both ways.
fn borrow_errors(ast: &Ast, names: &Names, typed: &Typed, mir: &Mir) -> Vec<BorrowError> {
    let mut errors = som_borrowcheck::check(mir);
    let generic = som_lower::lower_generic(ast, names, typed);
    errors.extend(som_borrowcheck::check(&generic));
    // The same error both ways names its types differently, `V` and `String`, so it's
    // compared without them.
    let mut seen = HashSet::new();
    errors.retain(|e| {
        let key = match e {
            BorrowError::UseAfterMove {
                name,
                span,
                moved_at,
                ..
            } => format!("UseAfterMove {name} {span:?} {moved_at:?}"),
            BorrowError::MoveOutOfDrop { name, span, .. } => {
                format!("MoveOutOfDrop {name} {span:?}")
            }
            other => format!("{other:?}"),
        };
        seen.insert(key)
    });
    errors
}

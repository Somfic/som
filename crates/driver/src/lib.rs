//! Runs the compiler: loads a program, then hands it through each pass in order.

mod api;
mod build;
mod error;
mod headers;
mod imports;
mod libraries;
mod load;
mod pipeline;
pub mod stdlib;
pub use api::{api, api_changes};
pub mod targets;
mod tests;
mod tokens;
pub mod toolchain;

pub use build::{BuildError, BuildOptions, build, command, emit_c, run};
pub use error::{Error, Warning};
pub use headers::{libc_bindings, origins};
pub use imports::{Importable, importable, locate};
pub use libraries::{LibraryError, LibraryWarning, SameAsHeader};
pub use load::{LoadError, load};
pub use som_lexer::highlight;
pub use tests::{FailedAt, Failure, Outcome, TestResult, TestRun, TestRunError, run_tests};
pub use tokens::dump_tokens;

use pipeline::{OnError, Passes, Stopped};
use som_ast::Ast;
use som_mir::Mir;
use som_resolve::Names;
use som_typeck::Typed;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use toolchain::Toolchain;

/// Everything the front end knows about a program that passed all checks so far.
pub struct Checked {
    pub ast: Ast,
    pub names: Names,
    pub typed: Typed,
    pub mir: Mir,
    /// The library files to link: those of the `extern "…"` blocks whose functions the
    /// program still uses after tree shaking.
    pub libraries: Vec<PathBuf>,
    /// What it was checked for, and so has to be built with.
    pub toolchain: Toolchain,
    pub warnings: Vec<Warning>,
    /// Each shader, compiled for the GPU, in the order they're written.
    pub shaders: Vec<Shader>,
}

/// A `@fragment` or `@compute` function, compiled for the GPU.
pub struct Shader {
    /// The function's name, without its module: `stripes`.
    pub name: String,
    pub stage: som_lang::Stage,
    /// Its WGSL source, as `gpu::shader` or `gpu::kernel` gives it.
    pub wgsl: String,
}

/// Why a program didn't pass its checks, with whatever warnings were found on the way.
#[derive(Debug)]
pub struct Failed {
    pub errors: Vec<Error>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub toolchain: Toolchain,
    /// Files' contents to use instead of what's on disk: an editor's unsaved changes.
    pub overlay: HashMap<PathBuf, String>,
    /// The file an editor has open: its module is loaded too, even if nothing uses it yet,
    /// so the file is checked as part of the program.
    pub opened: Option<PathBuf>,
    /// Leave the MIR as lowering made it, once constants are worked out: no folding, and
    /// nothing unused removed. To see what the program is before the optimizer changes it.
    pub no_optimize: bool,
    /// Check for every system, as warnings, the way an editor shows a program; otherwise
    /// only for the toolchain's target, where what it can't do is an error.
    pub every_system: bool,
    /// Build the program's tests: it starts at `som::run_tests`, which runs each `@test`,
    /// instead of at `main`.
    pub test: bool,
}

/// The folder of the program `file` belongs to: the nearest one around it, its own
/// included, with a `main.som`. A file in none is a program of its own, in its folder.
pub fn program_root(file: &Path) -> PathBuf {
    let folder = file.parent().unwrap_or(file);
    (folder.ancestors())
        .find(|dir| dir.join(toolchain::MAIN_FILE).is_file())
        .unwrap_or(folder)
        .to_path_buf()
}

/// Loads `path` and runs every check, stopping after the first pass that reports errors.
pub fn check(path: &Path, options: &Options) -> Result<Checked, Failed> {
    let (ast, errors) = load(path, &options.toolchain);
    if !errors.is_empty() {
        return Err(Failed {
            errors: errors.into_iter().map(Error::Load).collect(),
            warnings: Vec::new(),
        });
    }
    check_loaded(ast, options)
}

/// Runs every check on a program that loaded without errors.
pub fn check_loaded(ast: Ast, options: &Options) -> Result<Checked, Failed> {
    let mut passes = Passes::new(options, OnError::Stop);
    match check_passes(ast, options, &mut passes) {
        Ok(checked) => Ok(Checked {
            warnings: passes.warnings,
            ..checked
        }),
        Err(Stopped) => Err(Failed {
            errors: passes.errors,
            warnings: passes.warnings,
        }),
    }
}

fn check_passes(ast: Ast, options: &Options, passes: &mut Passes) -> Result<Checked, Stopped> {
    let front = passes.front(ast)?;
    let mut mir = passes.back(&front)?.expect("no errors, so lowered");
    // Before the optimizer shakes out the shaders nothing uses.
    let shaders = passes.shaders(&mut mir)?;
    if options.test
        && let Some(tests) = mir.tests
    {
        for func in mir.funcs.iter_mut() {
            func.is_entry = false;
        }
        mir.funcs[tests].is_entry = true;
    }
    if !options.no_optimize {
        som_optimize::optimize(&mut mir);
    }
    // Tree shaking may have removed every function a library was declared for.
    let used: HashSet<&PathBuf> = mir
        .funcs
        .iter()
        .filter_map(|f| f.library.as_ref())
        .collect();
    let libraries = (front.libraries.into_iter())
        .filter(|l| used.contains(l))
        .collect();
    Ok(Checked {
        ast: front.ast,
        names: front.names,
        typed: front.typed,
        mir,
        libraries,
        toolchain: options.toolchain.clone(),
        warnings: Vec::new(),
        shaders,
    })
}

/// Everything known about a program, even one with errors. For editor tooling,
/// which needs names and types for the parts that do make sense.
pub struct Analysis {
    pub ast: Ast,
    pub names: Names,
    pub typed: Typed,
    /// Only present when every check before lowering passed.
    pub mir: Option<Mir>,
    pub errors: Vec<Error>,
    pub warnings: Vec<Warning>,
}

/// Loads `path` and runs as many passes as possible. Loading, name resolution and
/// type checking always run; lowering and borrow checking only if they found nothing.
pub fn analyze(path: &Path, options: &Options) -> Analysis {
    let (ast, load_errors) = load::load_with(
        path,
        &options.toolchain,
        &options.overlay,
        options.opened.as_deref(),
    );
    let mut passes = Passes::new(options, OnError::Continue);
    let continuing = "the pipeline doesn't stop when continuing past errors";
    let _ = passes.report(load_errors.into_iter().map(Error::Load));
    let Ok(front) = passes.front(ast) else {
        unreachable!("{continuing}")
    };
    let Ok(mir) = passes.back(&front) else {
        unreachable!("{continuing}")
    };
    // Shaders are checked as written, as the editor shows them.
    let mut mir = mir;
    if let Some(mir) = &mut mir
        && passes.errors.is_empty()
    {
        let Ok(_) = passes.shaders(mir) else {
            unreachable!("{continuing}")
        };
    }
    Analysis {
        ast: front.ast,
        names: front.names,
        typed: front.typed,
        mir,
        errors: passes.errors,
        warnings: passes.warnings,
    }
}

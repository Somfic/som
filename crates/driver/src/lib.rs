//! Runs the compiler: loads a program, then hands it through each pass in order.

mod build;
mod headers;
mod libraries;
mod load;
pub mod stdlib;

pub use build::{BuildError, BuildOptions, build, emit_c, run};
pub use headers::origins;
pub use libraries::{LibraryError, LibraryWarning, SameAsHeader};
pub use load::{LoadError, load};

use som_ast::Ast;
use som_base::{Diagnostic, Fix, message, msg};
use som_borrowcheck::BorrowError;
use som_mir::Mir;
use som_resolve::{Names, ResolveError};
use som_typeck::{TypeError, Typed};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Everything the front end knows about a program that passed all checks so far.
pub struct Checked {
    pub ast: Ast,
    pub names: Names,
    pub typed: Typed,
    pub mir: Mir,
    /// The library files to link: those of the `extern "…"` blocks whose functions the
    /// program still uses after tree shaking.
    pub libraries: Vec<PathBuf>,
    pub warnings: Vec<Warning>,
}

/// Why a program didn't pass its checks, with whatever warnings were found on the way.
#[derive(Debug)]
pub struct Failed {
    pub errors: Vec<Error>,
    pub warnings: Vec<Warning>,
}

/// Settings for checking a program.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Extra directories to look for libraries in, before the system's.
    pub library_dirs: Vec<PathBuf>,
    /// Files' contents to use instead of what's on disk: an editor's unsaved changes.
    pub overlay: HashMap<PathBuf, String>,
    /// The file an editor has open: its module is loaded too, even if nothing uses it yet,
    /// so the file is checked as part of the program.
    pub opened: Option<PathBuf>,
    /// Leave the MIR as lowering made it, once constants are worked out: no folding, and
    /// nothing unused removed. To see what the program is before the optimizer changes it.
    pub no_optimize: bool,
}

/// The folder of the program `file` belongs to: the nearest one around it, its own
/// included, with a `main.som`. A file in none is a program of its own, in its folder.
pub fn program_root(file: &Path) -> PathBuf {
    let folder = file.parent().unwrap_or(file);
    (folder.ancestors())
        .find(|dir| dir.join("main.som").is_file())
        .unwrap_or(folder)
        .to_path_buf()
}

/// An error from any pass, keeping its pass-specific type.
#[derive(Debug)]
pub enum Error {
    Load(LoadError),
    Library(LibraryError),
    Resolve(ResolveError),
    Type(TypeError),
    Prove(som_prove::ProveError),
    Borrow(BorrowError),
    Comptime(som_comptime::ComptimeError),
}

impl Error {
    /// The name of the specific error, like `Mismatch` or `UseAfterMove`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Error::Load(e) => format!("{e:?}"),
            Error::Library(e) => format!("{e:?}"),
            Error::Resolve(e) => format!("{e:?}"),
            Error::Type(e) => format!("{e:?}"),
            Error::Prove(e) => format!("{e:?}"),
            Error::Borrow(e) => format!("{e:?}"),
            Error::Comptime(e) => format!("{e:?}"),
        };
        debug
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default()
            .to_string()
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Error::Load(e) => e.to_diagnostic(),
            Error::Library(e) => e.to_diagnostic(),
            Error::Resolve(e) => e.to_diagnostic(),
            Error::Type(e) => e.to_diagnostic(),
            Error::Prove(e) => e.to_diagnostic(),
            Error::Borrow(e) => e.to_diagnostic(),
            Error::Comptime(e) => e.to_diagnostic(),
        }
    }
}

/// Something worth fixing that doesn't stop a program compiling.
#[derive(Debug)]
pub enum Warning {
    Library(LibraryWarning),
}

impl Warning {
    /// The name of the specific warning, like `DeclaredInHeader`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Warning::Library(w) => format!("{w:?}"),
        };
        debug
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or_default()
            .to_string()
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            Warning::Library(w) => w.to_diagnostic(),
        }
    }
}

/// Checks types, working out first the constants needed to: `[i32; SIZE]` of
/// `@const SIZE = next_power_of_two(100);` needs `SIZE` run before the type is known. Each
/// round checks the program with what's worked out so far; the constants it needed are then
/// lowered (only what they run), borrow-checked and run, and it's checked again. A round
/// that works nothing new out is a constant needed to work itself out.
fn typecheck_rounds(ast: &Ast, names: &som_resolve::Names) -> (som_typeck::Typed, Vec<Error>) {
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
    names: &som_resolve::Names,
    typed: &som_typeck::Typed,
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

/// Whether a function in `mir` has a type that isn't known: made of a constant still to be
/// worked out.
fn mentions_unknown(mir: &som_mir::Mir) -> bool {
    fn unknown(mir: &som_mir::Mir, ty: som_mir::TypeId, depth: usize) -> bool {
        depth < 32
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

/// Loads `path` and runs every check, stopping after the first pass that reports errors.
/// The borrow checker's errors: in `mir`, the program as it's used, and in every generic
/// function with stand-ins for its type parameters, used or not, so a value used after it's
/// moved in one is found where it's written. Each once: a function the program uses with a
/// type that moves gives the same error both ways.
fn borrow_errors(
    ast: &som_ast::Ast,
    names: &som_resolve::Names,
    typed: &som_typeck::Typed,
    mir: &som_mir::Mir,
) -> Vec<BorrowError> {
    let mut errors = som_borrowcheck::check(mir);
    let generic = som_lower::lower_generic(ast, names, typed);
    errors.extend(som_borrowcheck::check(&generic));
    // The same error both ways, naming its types differently: `V`, and `String`.
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

pub fn check(path: &Path, options: &Options) -> Result<Checked, Failed> {
    let (ast, errors) = load(path, &options.library_dirs);
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
    let mut warnings = Vec::new();
    match check_passes(ast, options, &mut warnings) {
        Ok(checked) => Ok(Checked { warnings, ..checked }),
        Err(errors) => Err(Failed { errors, warnings }),
    }
}

/// Every pass after loading, adding the warnings they find to `warnings`.
fn check_passes(
    mut ast: Ast,
    options: &Options,
    warnings: &mut Vec<Warning>,
) -> Result<Checked, Vec<Error>> {
    // Missing functions and mismatched ones are reported together.
    let (libraries, mut errors) = libraries::check(&mut ast, &options.library_dirs);
    let (header_errors, header_warnings) = headers::check_declarations(&ast, &options.library_dirs);
    errors.extend(header_errors);
    warnings.extend(header_warnings.into_iter().map(Warning::Library));
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Library).collect());
    }

    let (names, mut errors) = som_resolve::resolve(&ast);
    if !errors.is_empty() {
        suggest_imports(&ast, &mut errors, options);
        return Err(errors.into_iter().map(Error::Resolve).collect());
    }

    let (typed, errors) = typecheck_rounds(&ast, &names);
    if !errors.is_empty() {
        return Err(errors);
    }

    let errors = som_prove::prove(&ast, &names, &typed);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Prove).collect());
    }

    let mut mir = som_lower::lower(&ast, &names, &typed);
    let errors = borrow_errors(&ast, &names, &typed, &mir);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Borrow).collect());
    }
    // Each `@const` that runs code, worked out by running it.
    let errors = som_comptime::run(&mut mir);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Comptime).collect());
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
    let libraries = libraries.into_iter().filter(|l| used.contains(l)).collect();

    Ok(Checked {
        ast,
        names,
        typed,
        mir,
        libraries,
        warnings: Vec::new(),
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

/// What a program can bring in that it hasn't yet: the standard library's modules (`use som;`) and
/// libraries with a header (`extern "raylib";`), each with the file it would link.
#[derive(Debug, Clone, Default)]
pub struct Importable {
    pub modules: Vec<String>,
    pub libraries: Vec<(String, PathBuf)>,
}

/// `raylib::init_window` without `extern "raylib";`: suggests importing it, when there's
/// a library or standard library module by that name.
fn suggest_imports(ast: &Ast, errors: &mut [ResolveError], options: &Options) {
    let mut found: Option<Importable> = None; // only read the directories when needed
    for error in errors {
        let (ResolveError::UnboundVariable {
            name,
            span,
            help,
            fixes,
        }
        | ResolveError::UnknownFunction {
            name,
            span,
            help,
            fixes,
        }
        | ResolveError::UnknownStruct {
            name,
            span,
            help,
            fixes,
        }
        | ResolveError::UnknownType {
            name,
            span,
            help,
            fixes,
        }) = error
        else {
            continue;
        };
        let Some((module, item)) = name.split_once("::") else {
            continue;
        };
        if ast.modules.iter().any(|m| *m.name == *module) {
            continue;
        }
        let found = found.get_or_insert_with(|| importable(&ast.modules[0].path, options));
        // `io::println` names `som::io` by its last part, once it's `use`d: the one loaded
        // with `println` in it, or the one there is to import.
        let last_part = |m: &str| m.rsplit("::").next() == Some(module);
        let has_item = |m: &som_ast::Module| {
            m.files.iter().any(|f| {
                let text = f.content();
                text.contains(&format!("fn {item}")) || text.contains(&format!("struct {item}"))
            })
        };
        let by_last_part = (ast.modules.iter())
            .find(|m| last_part(&m.name) && has_item(m))
            .map(|m| m.name.to_string())
            .or_else(|| {
                let mut named = found.modules.iter().filter(|m| last_part(m));
                match (named.next(), named.next()) {
                    (Some(only), None) => Some(only.clone()),
                    _ => None,
                }
            });
        let (line, shown) = if found.libraries.iter().any(|(l, _)| l == module) {
            (format!("extern {module:?};"), module.to_string())
        } else if found.modules.iter().any(|m| m == module) {
            (format!("use {module};"), module.to_string())
        } else if let Some(full) = by_last_part {
            (format!("use {full};"), full)
        } else {
            continue;
        };
        *help = Some(msg![
            message::module(&shown),
            " isn't imported: add ",
            message::code(&line)
        ]);
        fixes.push(Fix::add_line(
            msg!["add ", message::code(&line)],
            span.source.clone(),
            &line,
        ));
    }
}

/// Where `extern "library" from "header";` is found, for a program in `root`: the
/// library file and the header, when they are.
pub fn locate(
    library: Option<&str>,
    header: Option<&str>,
    root: &Path,
    options: &Options,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let library_file = library.and_then(|l| libraries::find(l, root, &options.library_dirs).ok());
    let header = header
        .map(str::to_string)
        .or_else(|| library.map(|l| format!("{l}.h")));
    let header_file =
        header.and_then(|h| headers::find_header(&h, root, &options.library_dirs).ok());
    (library_file, header_file)
}

/// What a program in `root` (a file or its folder) could import.
pub fn importable(root: &Path, options: &Options) -> Importable {
    let root = if root.is_dir() {
        root
    } else {
        root.parent().unwrap_or(Path::new("."))
    };
    Importable {
        modules: stdlib::modules(),
        libraries: headers::importable_libraries(root, &options.library_dirs),
    }
}

/// Loads `path` and runs as many passes as possible. Loading, name resolution and
/// type checking always run; lowering and borrow checking only if they found nothing.
pub fn analyze(path: &Path, options: &Options) -> Analysis {
    let (mut ast, load_errors) = load::load_with(
        path,
        &options.library_dirs,
        &options.overlay,
        options.opened.as_deref(),
    );
    let mut errors: Vec<Error> = load_errors.into_iter().map(Error::Load).collect();
    errors.extend(
        libraries::check(&mut ast, &options.library_dirs)
            .1
            .into_iter()
            .map(Error::Library),
    );
    let (header_errors, header_warnings) = headers::check_declarations(&ast, &options.library_dirs);
    errors.extend(header_errors.into_iter().map(Error::Library));
    let warnings = header_warnings.into_iter().map(Warning::Library).collect();

    let (names, mut resolve_errors) = som_resolve::resolve(&ast);
    suggest_imports(&ast, &mut resolve_errors, options);
    errors.extend(resolve_errors.into_iter().map(Error::Resolve));

    let (typed, type_errors) = typecheck_rounds(&ast, &names);
    let well_typed = type_errors.is_empty();
    errors.extend(type_errors);
    if well_typed && errors.is_empty() {
        errors.extend(
            som_prove::prove(&ast, &names, &typed)
                .into_iter()
                .map(Error::Prove),
        );
    }

    let mir = if errors.is_empty() {
        let mut mir = som_lower::lower(&ast, &names, &typed);
        errors.extend(
            borrow_errors(&ast, &names, &typed, &mir)
                .into_iter()
                .map(Error::Borrow),
        );
        // What's worked out while compiling, `@assert(…)` among it: its errors are the
        // program's too.
        if errors.is_empty() {
            errors.extend(som_comptime::run(&mut mir).into_iter().map(Error::Comptime));
        }
        Some(mir)
    } else {
        None
    };

    Analysis {
        ast,
        names,
        typed,
        mir,
        errors,
        warnings,
    }
}

/// Installs syntax highlighting for code snippets in rendered diagnostics.
pub fn enable_highlighting() {
    som_base::diagnostics::set_highlighter(som_lexer::highlight);
}

/// Every token of every file in the program, for `--emit tokens`. Files are named
/// by module and file name, so the dump doesn't depend on where the program lives.
pub fn dump_tokens(ast: &Ast) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for module in &ast.modules {
        for source in &module.files {
            let file = Path::new(source.identifier())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let name = if module.name.is_empty() {
                file.to_string()
            } else {
                format!("{}/{file}", module.name.replace("::", "/"))
            };
            let _ = writeln!(out, "file {name}");
            for token in som_lexer::lex(source.clone()) {
                if matches!(
                    token.kind,
                    som_lexer::TokenKind::Whitespace | som_lexer::TokenKind::Eof
                ) {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "  {}:{} {:?} {:?}",
                    token.span.start.line, token.span.start.col, token.kind, &*token.text
                );
            }
        }
    }
    out
}

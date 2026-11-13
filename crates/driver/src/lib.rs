//! Runs the compiler: loads a program, then hands it through each pass in order.

mod build;
mod bundled;
mod headers;
mod libraries;
mod load;

pub use build::{BuildError, BuildOptions, build, emit_c, run};
pub use headers::origins;
pub use libraries::LibraryError;
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
}

/// Settings for checking a program.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Extra directories to look for libraries in, before the system's.
    pub library_dirs: Vec<PathBuf>,
    /// Files' contents to use instead of what's on disk: an editor's unsaved changes.
    pub overlay: HashMap<PathBuf, String>,
}

/// An error from any pass, keeping its pass-specific type.
#[derive(Debug)]
pub enum Error {
    Load(LoadError),
    Library(LibraryError),
    Resolve(ResolveError),
    Type(TypeError),
    Borrow(BorrowError),
}

impl Error {
    /// The name of the specific error, like `Mismatch` or `UseAfterMove`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Error::Load(e) => format!("{e:?}"),
            Error::Library(e) => format!("{e:?}"),
            Error::Resolve(e) => format!("{e:?}"),
            Error::Type(e) => format!("{e:?}"),
            Error::Borrow(e) => format!("{e:?}"),
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
            Error::Borrow(e) => e.to_diagnostic(),
        }
    }
}

/// Loads `path` and runs every check, stopping after the first pass that reports errors.
pub fn check(path: &Path, options: &Options) -> Result<Checked, Vec<Error>> {
    let (ast, errors) = load(path, &options.library_dirs);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Load).collect());
    }
    check_loaded(ast, options)
}

/// Runs every check on a program that loaded without errors.
pub fn check_loaded(mut ast: Ast, options: &Options) -> Result<Checked, Vec<Error>> {
    // Missing functions and mismatched ones are reported together.
    let (libraries, mut errors) = libraries::check(&mut ast, &options.library_dirs);
    errors.extend(headers::check_declarations(&ast, &options.library_dirs));
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Library).collect());
    }

    let (names, mut errors) = som_resolve::resolve(&ast);
    if !errors.is_empty() {
        suggest_imports(&ast, &mut errors, options);
        return Err(errors.into_iter().map(Error::Resolve).collect());
    }

    let (typed, errors) = som_typeck::check(&ast, &names);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Type).collect());
    }

    let mut mir = som_lower::lower(&ast, &names, &typed);
    let errors = som_borrowcheck::check(&mir);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Borrow).collect());
    }
    som_optimize::optimize(&mut mir);
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
}

/// What a program can bring in that it hasn't yet: bundled modules (`use std;`) and
/// libraries with a header (`extern "raylib";`), each with the file it would link.
#[derive(Debug, Clone, Default)]
pub struct Importable {
    pub modules: Vec<String>,
    pub libraries: Vec<(String, PathBuf)>,
}

/// `raylib::init_window` without `extern "raylib";`: suggests importing it, when there's
/// a library or bundled module by that name.
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
        let Some((module, _)) = name.split_once("::") else {
            continue;
        };
        if ast.modules.iter().any(|m| *m.name == *module) {
            continue;
        }
        let found = found.get_or_insert_with(|| importable(&ast.modules[0].path, options));
        let line = if found.libraries.iter().any(|(l, _)| l == module) {
            format!("extern {module:?};")
        } else if found.modules.iter().any(|m| m == module) {
            format!("use {module};")
        } else {
            continue;
        };
        *help = Some(msg![
            message::module(module),
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
        modules: bundled::BUNDLED
            .iter()
            .map(|m| m.name.to_string())
            .collect(),
        libraries: headers::importable_libraries(root, &options.library_dirs),
    }
}

/// Loads `path` and runs as many passes as possible. Loading, name resolution and
/// type checking always run; lowering and borrow checking only if they found nothing.
pub fn analyze(path: &Path, options: &Options) -> Analysis {
    let (mut ast, load_errors) = load::load_with(path, &options.library_dirs, &options.overlay);
    let mut errors: Vec<Error> = load_errors.into_iter().map(Error::Load).collect();
    errors.extend(
        libraries::check(&mut ast, &options.library_dirs)
            .1
            .into_iter()
            .map(Error::Library),
    );
    errors.extend(
        headers::check_declarations(&ast, &options.library_dirs)
            .into_iter()
            .map(Error::Library),
    );

    let (names, mut resolve_errors) = som_resolve::resolve(&ast);
    suggest_imports(&ast, &mut resolve_errors, options);
    errors.extend(resolve_errors.into_iter().map(Error::Resolve));

    let (typed, type_errors) = som_typeck::check(&ast, &names);
    errors.extend(type_errors.into_iter().map(Error::Type));

    let mir = if errors.is_empty() {
        let mir = som_lower::lower(&ast, &names, &typed);
        errors.extend(som_borrowcheck::check(&mir).into_iter().map(Error::Borrow));
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

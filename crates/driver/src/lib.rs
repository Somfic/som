//! Runs the compiler: loads a program, then hands it through each pass in order.

mod build;
mod bundled;
mod load;

pub use build::{BuildError, BuildOptions, build, emit_c, run};
pub use load::{LoadError, load};

use som_ast::Ast;
use som_base::Diagnostic;
use som_borrowck::BorrowError;
use som_mir::Mir;
use som_resolve::{Names, ResolveError};
use som_typeck::{TypeError, Typed};
use std::path::Path;

/// Everything the front end knows about a program that passed all checks so far.
pub struct Checked {
    pub ast: Ast,
    pub names: Names,
    pub typed: Typed,
    pub mir: Mir,
}

/// An error from any pass, keeping its pass-specific type.
#[derive(Debug)]
pub enum Error {
    Load(LoadError),
    Resolve(ResolveError),
    Type(TypeError),
    Borrow(BorrowError),
}

impl Error {
    /// The name of the specific error, like `Mismatch` or `UseAfterMove`.
    pub fn kind(&self) -> String {
        let debug = match self {
            Error::Load(e) => format!("{e:?}"),
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
            Error::Resolve(e) => e.to_diagnostic(),
            Error::Type(e) => e.to_diagnostic(),
            Error::Borrow(e) => e.to_diagnostic(),
        }
    }
}

/// Loads `path` and runs every check, stopping after the first pass that reports errors.
pub fn check(path: &Path) -> Result<Checked, Vec<Error>> {
    let (ast, errors) = load(path);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Load).collect());
    }
    check_loaded(ast)
}

/// Runs every check on a program that loaded without errors.
pub fn check_loaded(ast: Ast) -> Result<Checked, Vec<Error>> {
    let (names, errors) = som_resolve::resolve(&ast);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Resolve).collect());
    }

    let (typed, errors) = som_typeck::check(&ast, &names);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Type).collect());
    }

    let mir = som_lower::lower(&ast, &names, &typed);
    let errors = som_borrowck::check(&mir);
    if !errors.is_empty() {
        return Err(errors.into_iter().map(Error::Borrow).collect());
    }

    Ok(Checked {
        ast,
        names,
        typed,
        mir,
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

/// Loads `path` and runs as many passes as possible. Loading, name resolution and
/// type checking always run; lowering and borrow checking only if they found nothing.
pub fn analyze(path: &Path) -> Analysis {
    let (ast, load_errors) = load(path);
    let mut errors: Vec<Error> = load_errors.into_iter().map(Error::Load).collect();

    let (names, resolve_errors) = som_resolve::resolve(&ast);
    errors.extend(resolve_errors.into_iter().map(Error::Resolve));

    let (typed, type_errors) = som_typeck::check(&ast, &names);
    errors.extend(type_errors.into_iter().map(Error::Type));

    let mir = if errors.is_empty() {
        let mir = som_lower::lower(&ast, &names, &typed);
        errors.extend(som_borrowck::check(&mir).into_iter().map(Error::Borrow));
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

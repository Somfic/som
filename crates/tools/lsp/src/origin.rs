//! Where a function, struct or constant imported from a C header is declared in it.

use std::path::{Path, PathBuf};

use som_ast::{Func, Id, Ident};
use som_driver::Analysis;
use som_resolve::{DefId, DefKind};
use tower_lsp::lsp_types::{Location, Position, Range, Url};

/// The folder next to a program that the som generated from its headers goes in.
const GENERATED_DIR: &str = ".som";

/// A header and a line in it.
pub(crate) type Origin = (PathBuf, usize);

pub(crate) fn of_func(a: &Analysis, f: Id<Func>) -> Option<Origin> {
    of_name(&a.ast.funcs[f].name)
}

/// Where `name`, declared in a file generated from a header, is declared in the header.
pub(crate) fn of_name(name: &Ident) -> Option<Origin> {
    let generated = PathBuf::from(name.span.source.identifier());
    let in_generated_dir = generated.parent()?.file_name()? == GENERATED_DIR;
    in_generated_dir.then_some(())?;
    som_driver::origins(&generated).remove(&*name.name)
}

pub(crate) fn of_def(a: &Analysis, def: DefId) -> Option<Origin> {
    match a.names.def(def).kind {
        DefKind::Func(f) => of_func(a, f),
        DefKind::Struct(s) => of_name(&a.ast.structs[s].name),
        _ => None,
    }
}

/// `file:///…/raylib.h#L968`.
pub(crate) fn header_url(header: &Path, line: usize) -> Option<Url> {
    let mut url = Url::from_file_path(header).ok()?;
    url.set_fragment(Some(&format!("L{line}")));
    Some(url)
}

pub(crate) fn location((header, line): Origin) -> Option<Location> {
    let position = Position::new(line.saturating_sub(1) as u32, 0);
    Some(Location {
        uri: Url::from_file_path(header).ok()?,
        range: Range::new(position, position),
    })
}

/// `doc`, followed by a link to where it's declared in C: `[raylib.h:968](…)`.
pub(crate) fn with_link(doc: Option<String>, origin: Option<Origin>) -> Option<String> {
    let Some((header, line)) = origin else {
        return doc;
    };
    let name = header.file_name()?.to_string_lossy();
    let link = format!("[{name}:{line}]({})", header_url(&header, line)?);
    Some(match doc {
        Some(doc) => format!("{doc}\n\n{link}"),
        None => link,
    })
}

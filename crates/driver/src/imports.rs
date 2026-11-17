use som_ast::Ast;
use som_base::{Fix, message, msg};
use som_resolve::ResolveError;
use std::path::{Path, PathBuf};

use crate::{Options, headers, libraries, stdlib};

/// What a program can bring in that it hasn't yet: the standard library's modules (`use som;`) and
/// libraries with a header (`extern "raylib";`), each with the file it would link.
#[derive(Debug, Clone, Default)]
pub struct Importable {
    pub modules: Vec<String>,
    pub libraries: Vec<(String, PathBuf)>,
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

/// `raylib::init_window` without `extern "raylib";`: suggests importing it, when there's
/// a library or standard library module by that name.
pub(crate) fn suggest_imports(ast: &Ast, errors: &mut [ResolveError], options: &Options) {
    // Only read the directories when needed.
    let mut found: Option<Importable> = None;
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
        let Some((line, shown)) = import_line(ast, found, module, item) else {
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

/// The line importing `module`, for `module::item`, and the module as it's shown.
fn import_line(
    ast: &Ast,
    found: &Importable,
    module: &str,
    item: &str,
) -> Option<(String, String)> {
    if found.libraries.iter().any(|(l, _)| l == module) {
        return Some((format!("extern {module:?};"), module.to_string()));
    }
    if found.modules.iter().any(|m| m == module) {
        return Some((format!("use {module};"), module.to_string()));
    }
    let full = by_last_part(ast, found, module, item)?;
    Some((format!("use {full};"), full))
}

/// `io::println` names `som::io` by its last part, once it's `use`d: the one loaded with
/// `println` in it, or the one there is to import.
fn by_last_part(ast: &Ast, found: &Importable, module: &str, item: &str) -> Option<String> {
    let last_part = |m: &str| m.rsplit("::").next() == Some(module);
    let has_item = |m: &som_ast::Module| {
        m.files.iter().any(|f| {
            let text = f.content();
            text.contains(&format!("fn {item}")) || text.contains(&format!("struct {item}"))
        })
    };
    (ast.modules.iter())
        .find(|m| last_part(&m.name) && has_item(m))
        .map(|m| m.name.to_string())
        .or_else(|| {
            let mut named = found.modules.iter().filter(|m| last_part(m));
            match (named.next(), named.next()) {
                (Some(only), None) => Some(only.clone()),
                _ => None,
            }
        })
}

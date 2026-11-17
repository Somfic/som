//! `extern "raylib" from "raylib.h";`: declarations imported from a C header.
//! `extern "raylib";` is short for exactly that.
//!
//! The header is read by clang, and its declarations are written out as som source, which
//! is parsed into a module named after the library: `raylib::init_window`, `raylib::Color`.
//! Everything after loading treats them as if they'd been written by hand.

use serde_json::Value;
use som_ast::{Ast, Ident, Item, Path as SomPath, Span, Use};
use som_base::Source;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::libraries;
use crate::{LoadError, toolchain};

mod cache;
mod check;
mod clang;
mod ctype;
mod docs;
mod emit;
mod macros;

pub use cache::origins;
pub use check::check_declarations;

/// `extern "c";`: the C standard library, whose functions go in module `c`. It's always
/// linked, so there's no library file to find, and its functions are spread over several
/// headers.
pub(crate) const LIBC: &str = "c";

/// Sets up the library modules for `module`'s `extern` blocks. A library's functions
/// go in a module named after it (`raylib::InitWindow`), whether they're written by
/// hand or imported from its header; `use extern` also `use`s that module. Headers are
/// imported once each, tracked in `imported`. Returns the names of the modules created.
pub fn import(
    ast: &mut Ast,
    module: usize,
    library_dirs: &[PathBuf],
    imported: &mut HashSet<String>,
    errors: &mut Vec<LoadError>,
) -> Vec<String> {
    let root = ast.modules[0].path.clone();
    let blocks: Vec<usize> = ast.modules[module]
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, Item::Extern(b) if b.library.is_some() || b.header.is_some()))
        .map(|(i, _)| i)
        .collect();

    let mut created = Vec::new();
    for index in blocks {
        let Item::Extern(block) = &ast.modules[module].items[index] else {
            unreachable!()
        };
        let (library, header, use_module) = (
            block.library.clone(),
            block.header.clone(),
            block.use_module,
        );
        let span = block
            .library_span
            .clone()
            .or_else(|| header.as_ref().map(|(_, s)| s.clone()));
        let name = module_name(library.as_deref(), header.as_ref().map_or("", |(h, _)| h));

        let target = match ast.modules.iter().position(|m| *m.name == name) {
            Some(existing) => existing,
            None => {
                created.push(name.clone());
                ast.add_module(name.as_str(), PathBuf::from(format!("<{name}>")))
            }
        };
        if let Item::Extern(block) = &mut ast.modules[module].items[index] {
            block.module = Some(target);
        }
        if use_module && let Some(span) = span {
            let path = SomPath {
                segments: vec![Ident {
                    name: name.as_str().into(),
                    span,
                }],
            };
            ast.modules[module].items.push(Item::Use(Use {
                path,
                public: false,
            }));
        }

        if let Some((header, span)) = header
            && imported.insert(name.clone())
        {
            let import = HeaderImport {
                library: library.as_deref(),
                header: &header,
                span,
                root: &root,
                library_dirs,
            };
            import.into_module(ast, target, errors);
        }
    }
    created
}

/// One `extern "library" from "header";`, to import.
struct HeaderImport<'a> {
    library: Option<&'a str>,
    header: &'a str,
    span: Span,
    root: &'a Path,
    library_dirs: &'a [PathBuf],
}

impl HeaderImport<'_> {
    /// Parses the header's declarations, as som, into `module`.
    fn into_module(self, ast: &mut Ast, module: usize, errors: &mut Vec<LoadError>) {
        let path = match self.find() {
            Ok(path) => path,
            Err(searched) => {
                errors.push(LoadError::HeaderNotFound {
                    header: self.header.to_string(),
                    span: self.span,
                    searched,
                });
                return;
            }
        };
        let library_file = (self.library)
            .and_then(|library| libraries::find(library, self.root, self.library_dirs).ok());
        let module_name = ast.modules[module].name.to_string();
        let cache = cache::Cache::new(self.root, &module_name, &path, library_file.as_deref());
        let text = match cache.read() {
            Some(text) => text,
            None => match self.generate(&path, library_file.as_deref()) {
                Ok((generated, origins)) => cache.write(&generated, &origins),
                Err(message) => {
                    errors.push(LoadError::HeaderFailed {
                        header: self.header.to_string(),
                        span: self.span,
                        message,
                    });
                    return;
                }
            },
        };
        ast.modules[module].path = cache.dir().to_path_buf();
        let source = Source::from_raw_at(text, cache.path);
        let parse_errors = som_parser::parse_file(Arc::new(source), ast, module);
        errors.extend(parse_errors.into_iter().map(LoadError::Parse));
        // C copies its structs whenever it passes them, and so does som.
        for item in &ast.modules[module].items {
            if let Item::Struct(s) = item {
                ast.structs.get_mut(*s).copy = true;
            }
        }
    }

    fn find(&self) -> Result<PathBuf, Vec<PathBuf>> {
        match self.library {
            Some(LIBC) => cache::libc_header(self.root).map_err(|_| Vec::new()),
            _ => find_header(self.header, self.root, self.library_dirs),
        }
    }

    /// The som source for the header at `path`, and its origins.
    fn generate(
        &self,
        path: &Path,
        library_file: Option<&Path>,
    ) -> Result<(String, String), String> {
        // Only what the library exports; if it can't be found, the library check reports that.
        let exported: Option<HashSet<String>> = library_file
            .and_then(|file| libraries::read_symbols(file).ok())
            .map(|names| names.into_iter().collect());
        // Without the `#define`s, there are no constants.
        let preprocessed =
            clang::preprocessed(path, self.root, self.library_dirs).unwrap_or_default();
        let json = clang::syntax_tree(path, self.root, self.library_dirs)?;
        Ok(emit::generate(
            &json,
            self.library,
            self.header,
            exported.as_ref(),
            &preprocessed,
        ))
    }
}

/// The declarations at the top of clang's JSON.
fn top_level(json: &Value) -> &[Value] {
    json["inner"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

/// The module a header's declarations go in: the library's name, or the header's.
fn module_name(library: Option<&str>, header: &str) -> String {
    let name = match library {
        Some(library) => Path::new(library)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(library),
        None => Path::new(header)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(header),
    };
    let name = name
        .strip_prefix(toolchain::LIBRARY_PREFIX)
        .filter(|_| library.is_some_and(|l| l.contains('/')))
        .unwrap_or(name);
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// `name`, with an `_` after it if it's reserved in som.
fn identifier(name: &str) -> String {
    if som_lexer::is_reserved(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// The header file, or the directories that were searched for it. A bare `name.h` is also
/// looked for as `name/name.h`, where packages like Homebrew's `miniaudio` put it.
pub(crate) fn find_header(
    header: &str,
    root: &Path,
    library_dirs: &[PathBuf],
) -> Result<PathBuf, Vec<PathBuf>> {
    let dirs = include_dirs(root, library_dirs);
    let nested = Path::new(header)
        .file_stem()
        .filter(|_| !header.contains('/'))
        .map(|stem| Path::new(stem).join(header));
    dirs.iter()
        .flat_map(|dir| [Some(dir.join(header)), nested.as_ref().map(|n| dir.join(n))])
        .flatten()
        .find(|path| path.is_file())
        .ok_or(dirs)
}

/// The libraries `extern "name";` could import for a program in `root`: those with
/// both a library file (`libraylib.dylib`) and a header (`raylib.h`) where som looks,
/// each with the file it would link.
pub fn importable_libraries(root: &Path, library_dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut names: Vec<String> = libraries::search_dirs(root, library_dirs)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| {
            let file = entry.ok()?.file_name().into_string().ok()?;
            toolchain::library_name(&file).map(str::to_string)
        })
        .collect();
    names.sort();
    names.dedup();
    names.retain(|name| find_header(&format!("{name}.h"), root, library_dirs).is_ok());
    names
        .into_iter()
        .filter_map(|name| {
            let path = libraries::find(&name, root, library_dirs).ok()?;
            Some((name, path))
        })
        .collect()
}

/// Where headers are looked for: the program's directory, then `CPATH` and
/// `C_INCLUDE_PATH`, then the `-isystem` and `-I` directories in Nix's
/// `NIX_CFLAGS_COMPILE` (packages like sqlite keep their headers apart from their
/// libraries), then the `include` directory next to each library directory.
fn include_dirs(root: &Path, library_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let root = toolchain::program_dir(root);
    let from_env = ["CPATH", "C_INCLUDE_PATH"]
        .into_iter()
        .filter_map(std::env::var_os)
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        // To clang an empty entry is the current directory; the program's comes first anyway.
        .filter(|dir| !dir.as_os_str().is_empty());
    let from_nix = std::env::var("NIX_CFLAGS_COMPILE")
        .map(|flags| libraries::flag_dirs(&flags, &["-isystem", "-I"]))
        .unwrap_or_default();
    let next_to_libraries = libraries::search_dirs(root, library_dirs)
        .into_iter()
        .filter_map(|dir| Some(dir.parent()?.join("include")));
    let mut dirs = vec![root.to_path_buf()];
    for include in from_env.chain(from_nix).chain(next_to_libraries) {
        if !dirs.contains(&include) {
            dirs.push(include);
        }
    }
    dirs
}

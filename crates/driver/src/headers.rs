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

use crate::LoadError;
use crate::libraries;
use crate::toolchain::{self, Toolchain};

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
    toolchain: &Toolchain,
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
        // The C library is the standard library's `som::c`, whose files declare it for
        // each system: `extern "c";` only `use`s it.
        if library.as_deref() == Some(LIBC) && header.is_some() {
            if use_module && let Some(span) = span {
                let segments = (som_lang::LIBC_MODULE.split("::"))
                    .map(|part| Ident {
                        name: part.into(),
                        span: span.clone(),
                    })
                    .collect();
                ast.modules[module].items.push(Item::Use(Use {
                    path: SomPath { segments },
                    public: false,
                }));
            }
            continue;
        }
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
                toolchain,
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
    toolchain: &'a Toolchain,
}

impl HeaderImport<'_> {
    /// Parses the header's declarations, as som, into `module`.
    fn into_module(self, ast: &mut Ast, module: usize, errors: &mut Vec<LoadError>) {
        let (text, path) = match self.import(&ast.modules[module].name) {
            Ok(loaded) => loaded,
            Err(error) => {
                errors.push(error);
                return;
            }
        };
        ast.modules[module].path = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let source = Source::from_raw_at(text, path);
        let parse_errors = som_parser::parse_file(Arc::new(source), ast, module);
        errors.extend(parse_errors.into_iter().map(LoadError::Parse));
        // C copies its structs whenever it passes them, and so does som.
        for item in &ast.modules[module].items {
            if let Item::Struct(s) = item {
                ast.structs.get_mut(*s).copy = true;
            }
        }
    }

    /// The header's declarations as som source, from the cache if they're still current,
    /// and where they're kept.
    fn import(&self, module_name: &str) -> Result<(String, PathBuf), LoadError> {
        let path = find_header(self.header, self.root, self.toolchain).map_err(|searched| {
            LoadError::HeaderNotFound {
                header: self.header.to_string(),
                span: self.span.clone(),
                searched,
            }
        })?;
        let library_file = (self.library)
            .and_then(|library| libraries::find(library, self.root, self.toolchain).ok());
        let cache = cache::Cache::new(
            self.root,
            &self.toolchain.target,
            module_name,
            &path,
            library_file.as_deref(),
        );
        let text = match cache.read() {
            Some(text) => text,
            None => {
                let (generated, origins) = (self.generate(&path, library_file.as_deref()))
                    .map_err(|message| LoadError::HeaderFailed {
                        header: self.header.to_string(),
                        span: self.span.clone(),
                        message,
                    })?;
                cache.write(&generated, &origins)
            }
        };
        Ok((text, cache.path))
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
        let preprocessed = clang::preprocessed(path, self.root, self.toolchain).unwrap_or_default();
        let json = clang::syntax_tree(path, self.root, self.toolchain)?;
        Ok(emit::generate(
            &json,
            self.library,
            self.header,
            exported.as_ref(),
            &preprocessed,
            &self.toolchain.target,
        ))
    }
}

/// The C library's declarations on `toolchain`'s target, as som source: what's kept in
/// the standard library's `c-library/` folder. `scratch` is where the header including
/// all of the C library's goes.
pub fn libc_bindings(scratch: &Path, toolchain: &Toolchain) -> Result<String, String> {
    let header = cache::libc_header(scratch, &toolchain.target).map_err(|e| e.to_string())?;
    let preprocessed = clang::preprocessed(&header, scratch, toolchain).unwrap_or_default();
    let json = clang::syntax_tree(&header, scratch, toolchain)?;
    let (generated, _) = emit::generate(
        &json,
        Some(LIBC),
        "c.h",
        None,
        &preprocessed,
        &toolchain.target,
    );
    Ok(generated)
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
    toolchain: &Toolchain,
) -> Result<PathBuf, Vec<PathBuf>> {
    let dirs = include_dirs(root, toolchain);
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
pub fn importable_libraries(root: &Path, toolchain: &Toolchain) -> Vec<(String, PathBuf)> {
    let mut names: Vec<String> = libraries::search_dirs(root, toolchain)
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
    names.retain(|name| find_header(&format!("{name}.h"), root, toolchain).is_ok());
    names
        .into_iter()
        .filter_map(|name| {
            let path = libraries::find(&name, root, toolchain).ok()?;
            Some((name, path))
        })
        .collect()
}

/// Where headers are looked for: the program's directory, then `CPATH` and
/// `C_INCLUDE_PATH`, then the `-isystem` and `-I` directories in Nix's
/// `NIX_CFLAGS_COMPILE` (packages like sqlite keep their headers apart from their
/// libraries), then the `include` directory next to each library directory. The
/// environment's are only for the system som runs on.
fn include_dirs(root: &Path, toolchain: &Toolchain) -> Vec<PathBuf> {
    let root = toolchain::program_dir(root);
    let next_to_libraries = libraries::search_dirs(root, toolchain)
        .into_iter()
        .filter_map(|dir| Some(dir.parent()?.join("include")));
    let mut dirs = vec![root.to_path_buf()];
    if toolchain.target.is_cross() {
        for include in next_to_libraries {
            if !dirs.contains(&include) {
                dirs.push(include);
            }
        }
        return dirs;
    }
    let from_env = ["CPATH", "C_INCLUDE_PATH"]
        .into_iter()
        .filter_map(std::env::var_os)
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        // To clang an empty entry is the current directory; the program's comes first anyway.
        .filter(|dir| !dir.as_os_str().is_empty());
    let from_nix = std::env::var("NIX_CFLAGS_COMPILE")
        .map(|flags| libraries::flag_dirs(&flags, &["-isystem", "-I"]))
        .unwrap_or_default();
    for include in from_env.chain(from_nix).chain(next_to_libraries) {
        if !dirs.contains(&include) {
            dirs.push(include);
        }
    }
    dirs
}

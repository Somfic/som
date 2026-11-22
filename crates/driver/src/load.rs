//! Each `use a::b;` loads module `a::b`: the standard library's module of that name if
//! there is one, otherwise the `.som` files in `<folder of the using module>/a/b/`. Modules
//! are identified by name, so every `use b;` in a program refers to the same module.

use som_ast::{Ast, Item, Span};
use som_base::message as m;
use som_base::{Diagnostic, Label, Source, msg};
use som_parser::{ParseError, parse_file};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::toolchain::{self, Toolchain};
use crate::{stdlib, targets};

#[derive(Debug)]
pub enum LoadError {
    HeaderNotFound {
        header: String,
        span: Span,
        searched: Vec<PathBuf>,
    },
    /// The header exists, but clang couldn't read it.
    HeaderFailed {
        header: String,
        span: Span,
        message: String,
    },
    ModuleNotFound {
        name: String,
        path: PathBuf,
        span: Span,
        /// The modules next to where it was looked for, as they're written in a `use`.
        nearby: Vec<String>,
    },
    CircularDependency {
        name: String,
        span: Span,
    },
    NoStandardLibrary {
        searched: Vec<PathBuf>,
        span: Span,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    Parse(ParseError),
}

impl LoadError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            LoadError::ModuleNotFound {
                name,
                path,
                span,
                nearby,
            } => {
                let mut d =
                    Diagnostic::error(msg!["module ", m::module(name), " not found"]).with_label(
                        Label::primary(span.clone(), msg!["looked in ", m::path(path.display())]),
                    );
                // By the last part of the path: `som::ioo` is close to `som::io`, `som::vec`
                // isn't to `som::mem`.
                let last = |n: &str| n.rsplit("::").next().unwrap_or(n).to_string();
                let lasts: Vec<String> = nearby.iter().map(|n| last(n)).collect();
                let close = som_base::diagnostics::closest_match(&last(name), &lasts, 2)
                    .and_then(|l| nearby.iter().find(|n| last(n) == l));
                if let Some(close) = close {
                    d = d
                        .with_hint(msg!["did you mean ", m::module(close), "?"])
                        .with_fix(som_base::Fix::replace(
                            msg!["change to ", m::code(close)],
                            span.clone(),
                            close.to_string(),
                        ));
                } else if !nearby.is_empty() {
                    let mut hint = msg!["modules there: "];
                    for (i, module) in nearby.iter().enumerate() {
                        if i > 0 {
                            hint.push(", ");
                        }
                        hint.push(m::module(module));
                    }
                    d = d.with_hint(hint);
                }
                // What people look for in the standard library is at its root, for everyone.
                if stdlib::is_std_module(name) {
                    d = d.with_hint(msg![
                        m::code("List"),
                        ", ",
                        m::code("Map"),
                        ", ",
                        m::code("string"),
                        ", ",
                        m::code("Rc"),
                        " and the rest of ",
                        m::module(som_lang::PRELUDE_MODULE),
                        " itself need no ",
                        m::code("use")
                    ]);
                }
                d
            }
            LoadError::NoStandardLibrary { searched, span } => {
                let searched: Vec<String> =
                    searched.iter().map(|p| p.display().to_string()).collect();
                Diagnostic::error("cannot find the standard library")
                    .with_label(Label::primary(span.clone(), "needed here"))
                    .with_hint(msg![
                        "looked in ",
                        searched.join(", "),
                        "; set ",
                        m::code("SOM_STD"),
                        " to its folder"
                    ])
            }
            LoadError::CircularDependency { name, span } => {
                Diagnostic::error(msg!["module ", m::module(name), " uses itself"])
                    .with_label(Label::primary(span.clone(), "this completes a cycle"))
            }
            LoadError::Io { path, message } => Diagnostic::error(msg![
                "could not read ",
                m::path(path.display()),
                format!(": {message}")
            ]),
            LoadError::Parse(error) => error.to_diagnostic(),
            LoadError::HeaderNotFound {
                header,
                span,
                searched,
            } => {
                let mut looked = msg!["looked in "];
                for (i, dir) in searched.iter().enumerate() {
                    looked.push(if i == 0 { "" } else { ", " });
                    looked.push(m::path(dir.display()));
                }
                Diagnostic::error(msg!["cannot find header ", m::path(header)])
                    .with_label(Label::primary(span.clone(), "not found"))
                    .with_hint(looked)
            }
            LoadError::HeaderFailed {
                header,
                span,
                message,
            } => Diagnostic::error(msg!["cannot import ", m::path(header)])
                .with_label(Label::primary(span.clone(), message.clone())),
        }
    }
}

/// Loads the program at `path` (a file or a directory) and everything it uses.
pub fn load(path: &Path, toolchain: &Toolchain) -> (Ast, Vec<LoadError>) {
    load_with(path, toolchain, &HashMap::new(), None)
}

/// Like [`load`], reading files in `overlay` from there instead of from disk: an
/// editor's unsaved changes. With `opened`, a file in one of the program's folders, its
/// module is loaded too, whether anything uses it or not.
pub fn load_with(
    path: &Path,
    toolchain: &Toolchain,
    overlay: &HashMap<PathBuf, String>,
    opened: Option<&Path>,
) -> (Ast, Vec<LoadError>) {
    // `.build/imports/raylib.som` is generated for the program around the build folder, so
    // it's that program that gets loaded, not one of its own in `imports/`.
    let program = generated_for(path);
    let path = program.as_deref().unwrap_or(path);

    let mut loader = Loader {
        toolchain: toolchain.clone(),
        overlay: overlay.clone(),
        ..Loader::default()
    };
    // A file in the standard library is checked as part of it: the program is the standard
    // library itself, loaded once, as `som`, around an empty program in its folder (where
    // imported headers are cached, in `.build/imports/`).
    let in_std = stdlib::contains(path).then(stdlib::root).flatten();
    let root = loader.ast.add_module(
        "",
        in_std.map_or_else(|| folder_of(path), Path::to_path_buf),
    );

    if in_std.is_none() {
        if path.is_dir() {
            loader.load_dir(root, path);
        } else {
            loader.load_file(root, path);
        }
        loader.import_headers(root);
        loader.load_uses(root);
        // The open file's module, named after its folder within the program: `help`.
        if let Some(file) = opened
            && let Some(folder) = file.parent()
            && let Ok(within) = folder.strip_prefix(folder_of(path))
            && !within.as_os_str().is_empty()
        {
            let name: Vec<String> = (within.components())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            loader.load_module(name.join("::"), folder, nowhere());
        }
    }
    // The standard library is always there by its full name, `som::io::printf`, without a
    // `use`; `use som::io;` is for naming what's in it unqualified.
    if !loader.loaded.contains(som_lang::PRELUDE_MODULE) {
        loader.load_module(som_lang::PRELUDE_MODULE.into(), &folder_of(path), nowhere());
    }
    // `@for v in Kind { … }`: written out now that every module, and `Kind`, is loaded.
    let errors = som_parser::expand_reflected(&mut loader.ast);
    targets::decide(&mut loader.ast, toolchain.target.os());
    loader
        .errors
        .extend(errors.into_iter().map(LoadError::Parse));

    (loader.ast, loader.errors)
}

/// The program a `.build/imports/` folder (`.build/<triple>/imports/` for another system), or
/// a file in one, was generated for: the folder around `.build/`.
fn generated_for(path: &Path) -> Option<PathBuf> {
    let folder = std::path::absolute(folder_of(path)).ok()?;
    if folder.file_name()? != toolchain::IMPORTS_DIR {
        return None;
    }
    let build = (folder.ancestors().skip(1).take(2)).find(|dir| {
        dir.file_name()
            .is_some_and(|name| name == toolchain::BUILD_DIR)
    })?;
    build.parent().map(Path::to_path_buf)
}

/// Where a module no `use` asked for is used from.
fn nowhere() -> Span {
    Span::empty(Arc::new(Source::from_raw("")))
}

fn folder_of(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(Path::to_path_buf).unwrap_or_default()
    }
}

#[derive(Default)]
struct Loader {
    ast: Ast,
    errors: Vec<LoadError>,
    loaded: HashSet<String>,
    loading: HashSet<String>,
    toolchain: Toolchain,
    /// Library modules whose header has been imported.
    imported: HashSet<String>,
    overlay: HashMap<PathBuf, String>,
}

impl Loader {
    fn load_dir(&mut self, module: usize, folder: &Path) {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(folder) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_source(p))
                .collect(),
            Err(err) => {
                self.errors.push(LoadError::Io {
                    path: folder.to_path_buf(),
                    message: err.to_string(),
                });
                return;
            }
        };
        files.sort();
        for file in files {
            self.load_file(module, &file);
        }
    }

    fn load_file(&mut self, module: usize, file: &Path) {
        if let Some(text) = self.overlay.get(file) {
            let source = Source::from_raw_at(text.as_str(), file);
            return self.parse(module, source);
        }
        match Source::from_file(file) {
            Ok(source) => self.parse(module, source),
            Err(err) => self.errors.push(LoadError::Io {
                path: file.to_path_buf(),
                message: err.to_string(),
            }),
        }
    }

    fn parse(&mut self, module: usize, source: Source) {
        let errors = parse_file(Arc::new(source), &mut self.ast, module);
        self.errors.extend(errors.into_iter().map(LoadError::Parse));
    }

    fn load_uses(&mut self, module: usize) {
        let folder = self.ast.modules[module].path.clone();
        let uses: Vec<(String, PathBuf, Span)> = self.ast.modules[module]
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Use(u) => {
                    let path = u
                        .path
                        .segments
                        .iter()
                        .fold(folder.clone(), |p, s| p.join(&*s.name));
                    Some((u.path.to_string(), path, u.path.span()))
                }
                _ => None,
            })
            .chain(self.ast.modules[module].block_uses.iter().map(|path| {
                let folder = path
                    .segments
                    .iter()
                    .fold(folder.clone(), |p, s| p.join(&*s.name));
                (path.to_string(), folder, path.span())
            }))
            .collect();

        for (name, path, span) in uses {
            self.load_module(name, &path, span);
        }
    }

    /// Imports the C headers `module` names, before its `use`s are followed, so `use raylib;`
    /// finds the imported module.
    fn import_headers(&mut self, module: usize) {
        let (ast, toolchain, imported, errors) = (
            &mut self.ast,
            &self.toolchain,
            &mut self.imported,
            &mut self.errors,
        );
        for name in crate::headers::import(ast, module, toolchain, imported, errors) {
            self.loaded.insert(name);
        }
    }

    fn load_module(&mut self, name: String, folder: &Path, span: Span) {
        if self.loaded.contains(&name) {
            return;
        }
        if self.loading.contains(&name) {
            // The standard library's modules use each other; names resolve whatever the order.
            if stdlib::is_std_module(&name) {
                return;
            }
            self.errors
                .push(LoadError::CircularDependency { name, span });
            return;
        }

        let module = if let Some(folder) = stdlib::folder(&name) {
            let module = self.ast.add_module(name.as_str(), folder.clone());
            // `som::OS`, the system it's compiled for, written for each build.
            if name == som_lang::PRELUDE_MODULE {
                self.parse(module, targets::target_source(&self.toolchain.target));
            }
            // Every system's C library, the target's first.
            if name == som_lang::LIBC_MODULE {
                for source in targets::libc_sources(self.toolchain.target.os()) {
                    self.parse(module, source);
                }
            } else {
                self.load_dir(module, &folder);
            }
            module
        } else if stdlib::is_std_module(&name) && stdlib::root().is_none() {
            self.errors.push(LoadError::NoStandardLibrary {
                searched: stdlib::candidates(),
                span,
            });
            return;
        } else if folder.is_dir() {
            let module = self.ast.add_module(name.as_str(), folder.to_path_buf());
            self.load_dir(module, folder);
            module
        } else if let Some(library) = stdlib::library_folder(&name) {
            // Not one of the program's own: a library, `use wgpu;`.
            let module = self.ast.add_module(name.as_str(), library.clone());
            self.load_dir(module, &library);
            module
        } else {
            let nearby = nearby_modules(&name, folder);
            self.errors.push(LoadError::ModuleNotFound {
                name,
                path: folder.to_path_buf(),
                span,
                nearby,
            });
            return;
        };

        self.loading.insert(name.clone());
        self.import_headers(module);
        self.load_uses(module);
        self.loading.remove(&name);
        self.loaded.insert(name.clone());
        // A standard library module comes with the ones inside it: `use som;` gives
        // `som::data::String`.
        for child in stdlib::submodules(&name) {
            self.load_module(child, folder, span.clone());
        }
    }
}

/// The modules next to `name`, which was looked for in `folder`, as a `use` writes them: the
/// standard library's inside its parent (`som::io` beside `som::vec`), or the folders of
/// `.som` files beside `folder`.
fn nearby_modules(name: &str, folder: &Path) -> Vec<String> {
    let parent = name.rsplit_once("::").map(|(parent, _)| parent);
    if let Some(parent) = parent
        && stdlib::is_std_module(parent)
    {
        return stdlib::submodules(parent);
    }
    // `foo`, looked for from where the program is: beside it, here.
    let dir = match folder.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let prefix = parent.map(|p| format!("{p}::")).unwrap_or_default();
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter(|e| {
                    std::fs::read_dir(e.path())
                        .is_ok_and(|mut files| files.any(|f| f.is_ok_and(|f| is_source(&f.path()))))
                })
                .filter_map(|e| e.file_name().to_str().map(str::to_string))
                // Not `.som`, where generated bindings go: only what a `use` can name.
                .filter(|n| n.starts_with(|c: char| c.is_alphabetic() || c == '_'))
                .map(|n| format!("{prefix}{n}"))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

fn is_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == toolchain::SOURCE_EXTENSION)
}

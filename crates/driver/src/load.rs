//! Finds and parses every module a program uses.
//!
//! A program is either a single `.som` file or a directory of them. Each `use a::b;`
//! loads module `a::b`: the bundled module of that name if there is one (with the bundled
//! modules inside it), otherwise
//! the `.som` files in `<folder of the using module>/a/b/`. Modules are identified by
//! name, so every `use b;` in a program refers to the same module.

use som_ast::{Ast, Item, Span};
use som_base::message as m;
use som_base::{Diagnostic, Label, Source, msg};
use som_parser::{ParseError, parse_file};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bundled::bundled;

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
    },
    CircularDependency {
        name: String,
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
            LoadError::ModuleNotFound { name, path, span } => {
                Diagnostic::error(msg!["module ", m::module(name), " not found"]).with_label(
                    Label::primary(span.clone(), msg!["looked in ", m::path(path.display())]),
                )
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
pub fn load(path: &Path, library_dirs: &[PathBuf]) -> (Ast, Vec<LoadError>) {
    load_with(path, library_dirs, &HashMap::new())
}

/// Like [`load`], reading files in `overlay` from there instead of from disk: an
/// editor's unsaved changes.
pub fn load_with(
    path: &Path,
    library_dirs: &[PathBuf],
    overlay: &HashMap<PathBuf, String>,
) -> (Ast, Vec<LoadError>) {
    // `.som/raylib.som` is generated for the program around it, so it's that program that
    // gets loaded. Loaded on its own, it would import its header into `.som/.som/`.
    let program = generated_for(path);
    let path = program.as_deref().unwrap_or(path);

    let mut loader = Loader {
        library_dirs: library_dirs.to_vec(),
        overlay: overlay.clone(),
        ..Loader::default()
    };
    let root = loader.ast.add_module("", folder_of(path));

    if path.is_dir() {
        loader.load_dir(root, path);
    } else {
        loader.load_file(root, path);
    }
    loader.import_headers(root);
    loader.load_uses(root);
    // The standard library is always there by its full name, `som::io::printf`, without a
    // `use`; `use som::io;` is for naming what's in it unqualified.
    if !loader.loaded.contains("som") {
        let nowhere = Span::empty(Arc::new(Source::from_raw("")));
        loader.load_module("som".into(), &folder_of(path), nowhere);
    }

    (loader.ast, loader.errors)
}

/// The program a `.som` folder, or a file in one, was generated for: the folder around it.
fn generated_for(path: &Path) -> Option<PathBuf> {
    let folder = std::path::absolute(folder_of(path)).ok()?;
    let generated = folder.file_name().is_some_and(|name| name == ".som");
    generated.then(|| folder.parent().map(Path::to_path_buf))?
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
    library_dirs: Vec<PathBuf>,
    /// Library modules whose header has been imported.
    imported: HashSet<String>,
    /// Files' contents to use instead of what's on disk.
    overlay: HashMap<PathBuf, String>,
}

impl Loader {
    fn load_dir(&mut self, module: usize, folder: &Path) {
        let mut files: Vec<PathBuf> = match std::fs::read_dir(folder) {
            Ok(entries) => entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "som"))
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

    /// Loads every module that `module` uses, recursively.
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
                let folder = path.segments.iter().fold(folder.clone(), |p, s| p.join(&*s.name));
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
        let (ast, dirs, imported, errors) = (
            &mut self.ast,
            &self.library_dirs,
            &mut self.imported,
            &mut self.errors,
        );
        for name in crate::headers::import(ast, module, dirs, imported, errors) {
            self.loaded.insert(name);
        }
    }

    fn load_module(&mut self, name: String, folder: &Path, span: Span) {
        if self.loaded.contains(&name) {
            return;
        }
        if self.loading.contains(&name) {
            // The standard library's modules use each other; names resolve whatever the order.
            if bundled(&name).is_some() {
                return;
            }
            self.errors
                .push(LoadError::CircularDependency { name, span });
            return;
        }

        let module = if let Some(bundled) = bundled(&name) {
            let module = self
                .ast
                .add_module(name.as_str(), PathBuf::from(format!("<{name}>")));
            for (file, content) in bundled.files {
                let source = Source::from_raw_at(*content, format!("<{name}>/{file}"));
                self.parse(module, source);
            }
            module
        } else if folder.is_dir() {
            let module = self.ast.add_module(name.as_str(), folder.to_path_buf());
            self.load_dir(module, folder);
            module
        } else {
            self.errors.push(LoadError::ModuleNotFound {
                name,
                path: folder.to_path_buf(),
                span,
            });
            return;
        };

        self.loading.insert(name.clone());
        self.import_headers(module);
        self.load_uses(module);
        self.loading.remove(&name);
        self.loaded.insert(name.clone());
        // A bundled module comes with the ones inside it: `use som;` gives `som::data::String`.
        for child in crate::bundled::submodules(&name) {
            self.load_module(child.name.to_string(), folder, span.clone());
        }
    }
}

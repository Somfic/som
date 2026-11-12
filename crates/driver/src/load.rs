//! Finds and parses every module a program uses.
//!
//! A program is either a single `.som` file or a directory of them. Each `use a::b;`
//! loads module `a::b`: the bundled module of that name if there is one, otherwise
//! the `.som` files in `<folder of the using module>/a/b/`. Modules are identified by
//! name, so every `use b;` in a program refers to the same module.

use som_ast::{Ast, Item, Span};
use som_base::message as m;
use som_base::{Diagnostic, Label, Source, msg};
use som_parser::{ParseError, parse_file};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bundled::bundled;

#[derive(Debug)]
pub enum LoadError {
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
        }
    }
}

/// Loads the program at `path` (a file or a directory) and everything it uses.
pub fn load(path: &Path) -> (Ast, Vec<LoadError>) {
    let mut loader = Loader::default();
    let root = loader.ast.add_module("", folder_of(path));

    if path.is_dir() {
        loader.load_dir(root, path);
    } else {
        loader.load_file(root, path);
    }
    loader.load_uses(root);

    (loader.ast, loader.errors)
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
            .collect();

        for (name, path, span) in uses {
            self.load_module(name, &path, span);
        }
    }

    fn load_module(&mut self, name: String, folder: &Path, span: Span) {
        if self.loaded.contains(&name) {
            return;
        }
        if self.loading.contains(&name) {
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
        self.load_uses(module);
        self.loading.remove(&name);
        self.loaded.insert(name);
    }
}

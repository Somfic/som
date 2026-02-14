use std::{collections::HashSet, fs, io, path::PathBuf, sync::Arc};

use crate::{
    Ast, Decl, Diagnostic, Label, Source,
    parser::{self, AstBuilder, ParseError},
};

#[derive(Debug)]
pub enum ProgramError {
    Io(io::Error),
    Parse(Vec<ParseError>),
    CircularDependency(String),
    ModuleNotFound {
        name: String,
        path: PathBuf,
        span: Option<crate::Span>,
    },
}

impl ProgramError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            ProgramError::Io(err) => Diagnostic::error(format!("I/O error: {}", err)),
            ProgramError::Parse(errors) => {
                let mut diag = Diagnostic::error("Failed to parse module");
                for error in errors {
                    diag =
                        diag.with_label(Label::primary(error.span.clone(), error.message.clone()));
                }
                diag
            }
            ProgramError::CircularDependency(module) => Diagnostic::error(format!(
                "Circular dependency detected while loading module `{}`",
                module
            )),
            ProgramError::ModuleNotFound { name, path, span } => {
                let diag = Diagnostic::error(format!("module `{}` could not be found", name));
                match span {
                    Some(s) => diag
                        .with_label(Label::primary(s.clone(), "unknown module"))
                        .with_hint(format!("cannot find source code in `{}`", path.display())),
                    None => diag.with_hint(format!("expected at `{}`", path.display())),
                }
            }
        }
    }
}

pub struct ProgramLoader {
    root: PathBuf,
    builder: AstBuilder,
    loaded: HashSet<String>,
    loading: HashSet<String>,
    errors: Vec<ProgramError>,
}

impl ProgramLoader {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            builder: AstBuilder::new(),
            loaded: HashSet::new(),
            loading: HashSet::new(),
            errors: Vec::new(),
        }
    }

    pub fn load_project(mut self) -> Result<Ast, Vec<ProgramError>> {
        let program_name = self
            .root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("main")
            .to_string();

        let root = self.root.clone();
        self.load_module(program_name, &root);

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(self.builder.finish())
    }

    fn discover_files(&self, folder: &std::path::Path) -> io::Result<Vec<PathBuf>> {
        let dir = fs::read_dir(folder)?;
        let mut files = dir
            .flatten()
            .filter(|f| f.file_name().to_string_lossy().ends_with(".som"))
            .map(|f| f.path())
            .collect::<Vec<_>>();

        files.sort();

        Ok(files)
    }

    fn load_module(&mut self, name: impl Into<String>, folder: &std::path::Path) {
        self.load_module_impl(name, folder, None);
    }

    fn load_module_with_span(
        &mut self,
        name: impl Into<String>,
        folder: &std::path::Path,
        span: crate::Span,
    ) {
        self.load_module_impl(name, folder, Some(span));
    }

    fn load_module_impl(
        &mut self,
        name: impl Into<String>,
        folder: &std::path::Path,
        span: Option<crate::Span>,
    ) {
        let name = name.into();

        if self.loaded.contains(&name) {
            return; // already loaded
        }

        if self.loading.contains(&name) {
            self.errors.push(ProgramError::CircularDependency(name));
            return;
        }

        // mark as being loaded
        self.loading.insert(name.clone());

        let files = match self.discover_files(folder) {
            Ok(files) => files,
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    self.errors.push(ProgramError::ModuleNotFound {
                        name: name.clone(),
                        path: folder.to_path_buf(),
                        span,
                    });
                } else {
                    self.errors.push(ProgramError::Io(err));
                }
                return;
            }
        };

        for file in files {
            let source = match Source::from_file(file.clone()) {
                Ok(source) => source,
                Err(err) => {
                    self.errors.push(ProgramError::Io(err));
                    continue;
                }
            };

            let source = Arc::new(source);
            let parse_errors = parser::parse_module(source, &mut self.builder, &name, file.clone());

            if !parse_errors.is_empty() {
                self.errors.push(ProgramError::Parse(parse_errors));
            }

            let deps: Vec<(String, PathBuf, crate::Span)> = {
                let module = self.builder.ast.mods.last().unwrap();
                module
                    .decs
                    .iter()
                    .filter_map(|decl| {
                        if let Decl::Use(use_id) = decl {
                            let use_stmt = self.builder.ast.uses.get(use_id);
                            let mut subfolder = folder.to_path_buf();
                            for level in use_stmt.path.0.iter() {
                                subfolder = subfolder.join(level.to_string());
                            }
                            Some((
                                use_stmt.path.to_string(),
                                subfolder,
                                use_stmt.path_span.clone(),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect()
            };

            for (dep_name, dep_path, span) in deps {
                self.load_module_with_span(dep_name, &dep_path, span);
            }
        }

        self.loading.remove(&name);
        self.loaded.insert(name);
    }
}

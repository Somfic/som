use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::PathBuf,
    sync::Arc,
};

use crate::{
    Module, Source,
    parser::{self, ParseError, Parser},
};

pub enum ModuleError {
    Io(io::Error),
    Parse(Vec<ParseError>),
    CircularDependency(String),
}

pub struct ModuleLoader {
    root: PathBuf,
    loaded: HashMap<String, Module>,
    loading: HashSet<String>,
    errors: Vec<ModuleError>,
}

impl ModuleLoader {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            loaded: HashMap::new(),
            loading: HashSet::new(),
            errors: Vec::new(),
        }
    }

    pub fn load_project(mut self) -> Result<Program, Vec<ModuleError>> {
        let program_name = self
            .root
            .iter()
            .last()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let root = self.root.clone();

        self.load_module(program_name.clone(), &root);

        if !self.errors.is_empty() {
            return Err(self.errors);
        }

        Ok(Program {
            root_module: program_name.to_string(),
            modules: self.loaded,
        })
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
        let name = name.into();

        if self.loaded.contains_key(&name) {
            return; // already loaded
        }

        if self.loading.contains(&name) {
            self.errors.push(ModuleError::CircularDependency(name));
            return;
        }

        // mark as being loaded
        self.loading.insert(name.clone());

        let files = match self.discover_files(folder) {
            Ok(files) => files,
            Err(err) => {
                self.errors.push(ModuleError::Io(err));
                return;
            }
        };

        for file in files {
            let source = match Source::from_file(file) {
                Ok(source) => source,
                Err(err) => {
                    self.errors.push(ModuleError::Io(err));
                    return;
                }
            };

            let source = Arc::new(source);
            let (ast, parse_errors) = parser::parse(source.clone());

            if !parse_errors.is_empty() {
                self.errors.push(ModuleError::Parse(parse_errors));
                return;
            }

            // find dependencies
            for depenency in ast.uses.iter() {
                let mut subfolder = folder.to_path_buf();
                for level in depenency.path.0.iter() {
                    subfolder = subfolder.join(level.to_string());
                }

                self.load_module(depenency.path.to_string(), &subfolder);
            }
        }

        let module = Module {
            name: name.into(),
            decs: ast,
            path: todo!(),
        };

        self.loading.remove(&name);
        self.loaded.insert(name, module);
    }
}

pub struct Program {
    pub root_module: String,
    pub modules: HashMap<String, Module>,
}

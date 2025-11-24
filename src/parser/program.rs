use crate::{ast::File, lexer::Path, Parser, Phase, Result, Source, Untyped};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug)]
pub struct Module<P: Phase> {
    pub path: Path,
    pub files: Vec<File<P>>,
}

pub struct ProgramParser {
    root: PathBuf,
}

impl ProgramParser {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn parse(&self) -> Result<Vec<Module<Untyped>>> {
        let source_files = find_som_files(&self.root);

        let mut modules: HashMap<Path, Vec<File<Untyped>>> = HashMap::new();
        for path in source_files {
            let module_path = {
                let mut segments = path.segments.clone();
                segments.pop(); // remove file name
                Path {
                    segments,
                    span: path.span.clone(),
                }
            };

            let source = Source::from_file(PathBuf::from(&path)).unwrap();
            let mut parser = Parser::new(source);
            let file = parser.parse()?;

            modules.entry(module_path).or_default().push(file);
        }

        Ok(modules
            .into_iter()
            .map(|(path, files)| Module { path, files })
            .collect())
    }
}

fn find_som_files(dir: &PathBuf) -> Vec<Path> {
    let mut som_files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();

            if entry_path.is_file()
                && entry_path.extension().and_then(|s| s.to_str()) == Some("som")
            {
                som_files.push(Path::from(&entry_path));
            } else if entry_path.is_dir() {
                som_files.extend(find_som_files(&entry_path));
            }
        }
    }

    som_files
}

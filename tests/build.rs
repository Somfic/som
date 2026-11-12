//! Generates one `#[test]` per case in `tests/corpus`.
//!
//! A case is either a single `.som` file, or a directory containing `main.som`
//! (a multi-file project). The expectation lives in the case's header comments.

#[allow(dead_code)] // the build script only needs the `ignore` part
#[path = "src/header.rs"]
mod header;

use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let corpus = corpus
        .canonicalize()
        .expect("the corpus directory should exist");
    println!("cargo:rerun-if-changed={}", corpus.display());

    let mut out = String::new();
    let mut categories: Vec<PathBuf> = read_sorted(&corpus)
        .into_iter()
        .filter(|p| p.is_dir())
        .collect();
    categories.sort();

    for category in categories {
        let name = ident(category.file_name().unwrap().to_str().unwrap());
        writeln!(out, "mod {name} {{").unwrap();
        for case in read_sorted(&category) {
            println!("cargo:rerun-if-changed={}", case.display());
            let main = if case.is_dir() {
                case.join("main.som")
            } else {
                case.clone()
            };
            if main.extension().is_none_or(|e| e != "som") || !main.exists() {
                continue;
            }
            let header = header::Header::parse(&fs::read_to_string(&main).unwrap());
            let test_name = ident(case.file_stem().unwrap().to_str().unwrap());
            if let Some(reason) = &header.ignore {
                writeln!(out, "    #[ignore = {reason:?}]").unwrap();
            }
            writeln!(out, "    #[test]").unwrap();
            writeln!(
                out,
                "    fn {test_name}() {{ crate::run_case({:?}); }}",
                case.display().to_string()
            )
            .unwrap();
        }
        writeln!(out, "}}").unwrap();
    }

    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("cases.rs");
    fs::write(dest, out).unwrap();
}

fn read_sorted(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    entries
}

fn ident(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

//! Bundles the standard library into the driver: every `.som` file under `std/`, in a module
//! named after its folder. `std/` itself is module `som`, `std/data/` is `som::data`, and so
//! on. Writes the table `src/bundled.rs` includes.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    let root = manifest.join("../../std");
    // Cargo looks through the whole folder: adding, removing or changing a file reruns this.
    println!("cargo:rerun-if-changed={}", root.display());

    let mut modules = Vec::new();
    collect(&root, "som".to_string(), &mut modules);

    let mut out = String::from("pub const BUNDLED: &[BundledModule] = &[\n");
    for (name, files) in &modules {
        writeln!(
            out,
            "    BundledModule {{\n        name: {name:?},\n        files: &["
        )
        .unwrap();
        for file in files {
            let file_name = file
                .file_name()
                .expect("files have names")
                .to_string_lossy();
            let path = file.canonicalize().expect("listed files exist");
            writeln!(
                out,
                "            ({file_name:?}, include_str!({:?})),",
                path.display().to_string()
            )
            .unwrap();
        }
        writeln!(out, "        ],\n    }},").unwrap();
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo")).join("bundled.rs");
    std::fs::write(dest, out).expect("OUT_DIR is writable");
}

/// Adds module `name`, the `.som` files in `folder`, then one for each folder inside it, in
/// name order so the table doesn't depend on the file system's.
fn collect(folder: &Path, name: String, modules: &mut Vec<(String, Vec<PathBuf>)>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|dir| dir.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    entries.sort();
    let files = (entries.iter())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "som"))
        .cloned()
        .collect();
    modules.push((name.clone(), files));
    for dir in entries.iter().filter(|p| p.is_dir()) {
        let child = dir
            .file_name()
            .expect("folders have names")
            .to_string_lossy();
        collect(dir, format!("{name}::{child}"), modules);
    }
}

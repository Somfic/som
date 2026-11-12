//! Every corpus program that isn't expected to fail (or marked as a known failure)
//! must parse without errors.

use som_base::Source;
use std::path::Path;
use std::sync::Arc;

#[test]
fn corpus_parses() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/corpus");
    let mut failures = Vec::new();
    let mut count = 0;

    for file in som_files(&corpus) {
        let source = Source::from_file(&file).unwrap();
        let header = source.content();
        if header.starts_with("// expect: error") || header.contains("\n// ignore:") {
            continue;
        }
        count += 1;
        let (_, errors) = som_parser::parse(Arc::new(source));
        if !errors.is_empty() {
            failures.push(format!("{}: {:?}", file.display(), errors[0].message));
        }
    }

    assert!(count > 0, "no corpus files found");
    assert!(
        failures.is_empty(),
        "{} of {count} files failed to parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn som_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(som_files(&path));
        } else if path.extension().is_some_and(|e| e == "som") {
            files.push(path);
        }
    }
    files
}

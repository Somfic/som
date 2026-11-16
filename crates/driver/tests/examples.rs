//! `example/errors/`: one program for each compile-time code error, each failing with the
//! error its file is named for (`ConstFailed.som`, `ConstFailed_fail.som`, `Parse_….som`).

use std::path::Path;

#[test]
fn each_error_example_fails_with_its_error() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/errors");
    let mut wrong = Vec::new();
    let mut count = 0;
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("example/errors exists")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "som"))
        .collect();
    files.sort();
    for file in files {
        count += 1;
        let stem = file.file_stem().unwrap().to_string_lossy().into_owned();
        let kind = stem.split('_').next().unwrap();
        let found = match som_driver::check(&file, &Default::default()) {
            Ok(_) => "compiles".to_string(),
            Err(failed) => (failed.errors.iter().map(|e| e.kind()))
                .collect::<Vec<_>>()
                .join(", "),
        };
        if found.split(", ").next() != Some(kind) {
            wrong.push(format!("{stem}: {found}"));
        }
    }
    assert!(count > 0, "no examples found");
    assert!(wrong.is_empty(), "examples failing differently:\n{}", wrong.join("\n"));
}

//! Which program a file belongs to, as an editor opening it sees it.

use som_driver::{Options, analyze, program_root};

#[test]
fn a_file_in_a_subfolder_belongs_to_the_program_around_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("main.som"), "fn main() -> i32 { 0 }\n").unwrap();
    std::fs::create_dir(root.join("help")).unwrap();
    let file = root.join("help").join("test.som");
    std::fs::write(&file, "pub fn broken() -> i32 { true }\n").unwrap();

    // The nearest folder with a `main.som`, not the file's own.
    assert_eq!(program_root(&file), root);

    // Nothing uses `help`, but the open file's module is checked anyway.
    let options = Options {
        opened: Some(file.clone()),
        ..Default::default()
    };
    let analysis = analyze(&program_root(&file), &options);
    assert!(
        matches!(&analysis.errors[..], [e] if e.kind() == "Mismatch"),
        "the `bool` returned as an `i32` in test.som: {:?}",
        analysis.errors
    );
    // Without it open, `help` isn't part of the program.
    let unopened = analyze(&program_root(&file), &Options::default());
    assert!(unopened.errors.is_empty(), "{:?}", unopened.errors);
}

#[test]
fn a_file_outside_any_program_is_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("lone.som");
    std::fs::write(&file, "fn main() -> i32 { 0 }\n").unwrap();
    assert_eq!(program_root(&file), dir.path());
}

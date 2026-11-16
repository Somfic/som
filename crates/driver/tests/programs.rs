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

#[test]
fn checks_run_while_compiling_are_reported_to_an_editor() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.som");
    std::fs::write(
        &file,
        "@const TEST = 12;\n@assert(TEST == 0, \"TEST is not 0\");\nfn main() -> i32 = 0;\n",
    )
    .unwrap();
    let analysis = analyze(&program_root(&file), &Options::default());
    assert!(
        matches!(&analysis.errors[..], [e] if e.kind() == "ConstFailed"),
        "the failed `@assert`: {:?}",
        analysis.errors
    );
}

#[test]
fn an_unknown_built_in_in_an_at_for_is_the_only_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.som");
    std::fs::write(
        &file,
        "enum Kind { A, B, C }\n\
         fn index(k: Kind) -> usize = match k {\n    @for (i, v) in @variants(Kind) { v => i, }\n};\n\
         fn main() -> i32 = 0;\n",
    )
    .unwrap();
    // What an editor's told: the same as `som build`, nothing more.
    let analysis = analyze(&program_root(&file), &Options::default());
    assert_eq!(analysis.errors.len(), 1, "{:?}", analysis.errors);
    assert!(
        format!("{}", analysis.errors[0].to_diagnostic()).contains("unknown built-in"),
        "{:?}",
        analysis.errors
    );
}

#[test]
fn an_editor_works_out_constants_used_in_types() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.som");
    std::fs::write(
        &file,
        "fn size() -> usize = 2 * 64;\n\
         @const SIZE = size();\n\
         struct Ring { slots: [i32; SIZE] }\n\
         fn main() -> i32 { let ring = Ring { slots: [0; SIZE] }; ring.slots[127] }\n",
    )
    .unwrap();
    // As `som build` sees it: `SIZE` is worked out, so `[i32; SIZE]` has 128 slots.
    let analysis = analyze(&program_root(&file), &Options::default());
    assert!(analysis.errors.is_empty(), "{:?}", analysis.errors);
}

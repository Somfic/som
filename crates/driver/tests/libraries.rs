//! `extern "library"` checks, against small libraries compiled with the system C compiler.

use som_driver::{Error, LibraryError, Options, check};
use std::path::Path;
use std::process::Command;

const C_SOURCE: &str = "int demo_add(int a, int b) { return a + b; }\n";

/// The file name a shared library gets on this system.
fn shared(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{name}.dylib")
    } else {
        format!("lib{name}.so")
    }
}

/// Compiles `C_SOURCE` into a shared library `name` in `dir`.
fn shared_library(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("demo.c"), C_SOURCE).unwrap();
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(dir.join(shared(name)))
        .arg(dir.join("demo.c"))
        .status()
        .unwrap();
    assert!(status.success());
}

fn program(dir: &Path, library: &str, function: &str) -> std::path::PathBuf {
    let path = dir.join("main.som");
    let source = format!(
        "use extern \"{library}\" {{ fn {function}(a: i32, b: i32) -> i32; }}\nfn main() -> i32 {{ {function}(40, 2) }}\n"
    );
    std::fs::write(&path, source).unwrap();
    path
}

fn library_errors(result: Result<som_driver::Checked, Vec<Error>>) -> Vec<LibraryError> {
    match result {
        Ok(_) => Vec::new(),
        Err(errors) => errors
            .into_iter()
            .map(|e| match e {
                Error::Library(e) => e,
                other => panic!("unexpected {other:?}"),
            })
            .collect(),
    }
}

#[test]
fn a_library_next_to_the_program_is_found_linked_and_run() {
    let dir = tempfile::tempdir().unwrap();
    shared_library(dir.path(), "demo");
    let main = program(dir.path(), "demo", "demo_add");

    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(checked.libraries, [dir.path().join(shared("demo"))]);
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    let status = som_driver::run(&checked, &exe, &[]).unwrap();
    assert_eq!(status.code(), Some(42));
}

#[test]
fn a_library_whose_functions_go_unused_is_not_linked() {
    let dir = tempfile::tempdir().unwrap();
    shared_library(dir.path(), "demo");
    let main = dir.path().join("main.som");
    std::fs::write(
        &main,
        "use extern \"demo\" { fn demo_add(a: i32, b: i32) -> i32; }\nfn main() -> i32 { 0 }\n",
    )
    .unwrap();

    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(checked.libraries.is_empty(), "{:?}", checked.libraries);
}

#[test]
fn a_function_the_library_lacks_is_reported_with_a_suggestion() {
    let dir = tempfile::tempdir().unwrap();
    shared_library(dir.path(), "demo");
    let main = program(dir.path(), "demo", "demo_ad");

    match &library_errors(check(&main, &Options::default()))[..] {
        [
            LibraryError::NotInLibrary {
                function,
                suggestion,
                ..
            },
        ] => {
            assert_eq!(function, "demo_ad");
            assert_eq!(suggestion.as_deref(), Some("demo_add"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn library_directories_are_searched() {
    let dir = tempfile::tempdir().unwrap();
    let libs = dir.path().join("libs");
    shared_library(&libs, "demo");
    let main = program(dir.path(), "demo", "demo_add");

    assert!(matches!(
        library_errors(check(&main, &Options::default()))[..],
        [LibraryError::LibraryNotFound { .. }]
    ));
    let options = Options {
        library_dirs: vec![libs],
        ..Options::default()
    };
    assert!(check(&main, &options).is_ok());
}

#[test]
fn a_misspelled_library_gets_a_suggestion() {
    let dir = tempfile::tempdir().unwrap();
    shared_library(dir.path(), "demo");
    let main = program(dir.path(), "deom", "demo_add");

    match &library_errors(check(&main, &Options::default()))[..] {
        [LibraryError::LibraryNotFound { suggestion, .. }] => {
            assert_eq!(suggestion.as_deref(), Some("demo"))
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn static_libraries_are_read_too() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("demo.c"), C_SOURCE).unwrap();
    let object = dir.path().join("demo.o");
    assert!(
        Command::new("cc")
            .arg("-c")
            .arg(dir.path().join("demo.c"))
            .arg("-o")
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("ar")
            .arg("rcs")
            .arg(dir.path().join("libstat.a"))
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );

    let main = program(dir.path(), "stat", "demo_add");
    assert!(check(&main, &Options::default()).is_ok());
    let main = program(dir.path(), "stat", "demo_sub");
    assert!(matches!(
        library_errors(check(&main, &Options::default()))[..],
        [LibraryError::NotInLibrary { .. }]
    ));
}

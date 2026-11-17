//! `run_tests`: a program's `@test`s built, run, and read back, as the editor shows them.

use som_driver::{Options, Outcome, TestRunError, run_tests};
use std::time::Duration;

fn program(source: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.som"), source).unwrap();
    dir
}

#[test]
fn each_test_passes_or_fails_with_why() {
    let dir = program(
        "@test\nfn adds() {\n    assert(1 + 1 == 2);\n}\n\n\
         @test\nfn subtracts() {\n    assert(2 - 1 == 0, \"off by one\");\n}\n",
    );
    let run = run_tests(dir.path(), &Options::default(), Duration::from_secs(30))
        .expect("the tests build");
    assert!(!run.timed_out);
    let outcomes: Vec<(&str, &Outcome)> = (run.results.iter())
        .map(|r| (r.name.as_str(), &r.outcome))
        .collect();
    assert_eq!(outcomes[0], ("adds", &Outcome::Passed));
    assert_eq!(outcomes[1].0, "subtracts");
    assert!(
        matches!(outcomes[1].1, Outcome::Failed(f) if f.why == "off by one"
            && f.at.as_ref().is_some_and(|at| at.line == 8)),
        "{outcomes:?}"
    );
}

#[test]
fn a_test_that_never_ends_is_stopped() {
    let dir = program(
        "@test\nfn passes() {}\n\n\
         @test\nfn spins() {\n    let mut n = 0;\n    while true {\n        n = n + 1;\n    }\n}\n\n\
         @test\nfn after() {}\n",
    );
    let run = run_tests(dir.path(), &Options::default(), Duration::from_secs(2))
        .expect("the tests build");
    assert!(run.timed_out);
    let names: Vec<&str> = run.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["passes"]);
}

#[test]
fn a_program_with_errors_isnt_run() {
    let dir = program("@test\nfn wrong() {\n    let x: i32 = \"text\";\n}\n");
    let ran = run_tests(dir.path(), &Options::default(), Duration::from_secs(30));
    assert!(matches!(ran, Err(TestRunError::Check(_))));
}

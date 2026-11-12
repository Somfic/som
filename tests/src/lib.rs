//! Runs every program in `corpus/` through the compiler and checks it against
//! its header. `build.rs` generates one test per case into the `cases` module.

mod header;

pub use header::{Expect, Header};

use som_driver::Error;
use std::path::Path;

pub fn run_case(path: &str) {
    let path = Path::new(path);
    let main = if path.is_dir() {
        path.join("main.som")
    } else {
        path.to_path_buf()
    };
    let header = Header::parse(&std::fs::read_to_string(&main).unwrap());

    let result = som_driver::check(path, &Default::default());

    if let Ok(checked) = &result {
        let problems = checked.mir.validate();
        assert!(
            problems.is_empty(),
            "invalid MIR (a compiler bug):\n{}",
            problems.join("\n")
        );
    }

    match (&header.expect, result) {
        (Expect::Ok, Ok(_)) => {}
        (Expect::Exit(expected), Ok(checked)) => {
            let dir = tempfile::tempdir().unwrap();
            let exe = som_driver::build(&checked, &dir.path().join("program"), &Default::default())
                .unwrap_or_else(|e| panic!("build failed:\n{}", e.to_diagnostic()));
            let status = som_driver::run(&checked, &exe, &[]).expect("could not start the program");
            let code = status.code().expect("program was killed by a signal");
            // Exit codes are truncated to a byte by the OS.
            assert_eq!(
                code,
                expected & 0xff,
                "wrong exit code for {}",
                path.display()
            );
        }
        (Expect::Error(kinds), Err(errors)) => {
            for kind in kinds {
                assert!(
                    errors.iter().any(|e| &e.kind() == kind),
                    "expected a `{kind}` error, got:\n{}",
                    describe(&errors)
                );
            }
        }
        (Expect::Error(_), Ok(_)) => {
            panic!("expected an error, but {} compiled cleanly", path.display())
        }
        (_, Err(errors)) => panic!("expected success, got errors:\n{}", describe(&errors)),
    }
}

fn describe(errors: &[Error]) -> String {
    errors
        .iter()
        .map(|e| format!("{}", e.to_diagnostic()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod cases {
    include!(concat!(env!("OUT_DIR"), "/cases.rs"));
}

//! Runs the real `som` binary.

use std::path::Path;
use std::process::{Command, Output};

const PROGRAM: &str = "fn main() -> i32 {\n    let x = 40;\n    x + 2\n}\n";

fn som(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_som"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("som runs")
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("main.som"), PROGRAM).unwrap();
    dir
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn help_lists_commands_in_workflow_order() {
    let help = stdout(&som(Path::new("."), &["--help"]));
    let positions: Vec<usize> = ["  run ", "  build ", "  check "]
        .iter()
        .map(|c| help.find(c).expect(c))
        .collect();
    assert!(positions.is_sorted(), "{help}");
}

#[test]
fn run_exits_with_the_programs_exit_code() {
    let dir = project();
    let out = som(dir.path(), &["run", "main.som"]);
    assert_eq!(out.status.code(), Some(42));
    assert!(String::from_utf8_lossy(&out.stderr).contains("built: `main.som`"));
}

#[test]
fn build_writes_an_executable_named_after_the_file() {
    let dir = project();
    assert!(som(dir.path(), &["build", "main.som"]).status.success());
    let status = Command::new(dir.path().join("main")).status().unwrap();
    assert_eq!(status.code(), Some(42));
}

#[test]
fn emit_writes_stages_next_to_the_output() {
    let dir = project();
    let out = som(
        dir.path(),
        &[
            "build",
            "main.som",
            "-o",
            "app",
            "--emit",
            "tokens,ast,mir,c",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for ext in ["tokens", "ast", "mir", "c"] {
        assert!(
            dir.path().join(format!("app.{ext}")).exists(),
            "missing app.{ext}"
        );
    }
}

#[test]
fn emit_to_stdout_and_to_a_named_file() {
    let dir = project();
    let out = som(
        dir.path(),
        &["check", "main.som", "--emit", "mir=-,c=generated.c"],
    );
    assert!(
        stdout(&out).starts_with("fn main() -> i32 {"),
        "{}",
        stdout(&out)
    );
    assert!(dir.path().join("generated.c").exists());
}

#[test]
fn tokens_and_ast_are_written_even_when_checking_fails() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("bad.som"), "fn main() -> i32 { true }").unwrap();
    let out = som(dir.path(), &["check", "bad.som", "--emit", "ast=-,mir=-"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        stdout(&out),
        "module \"\"\n  fn main() -> i32\n    block\n      value\n        bool true\n"
    );
}

#[test]
fn unknown_stage_is_a_usage_error() {
    let dir = project();
    let out = som(dir.path(), &["build", "main.som", "--emit", "llvm"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown stage `llvm`"));
}

#[test]
fn verbose_prints_the_c_compiler_command() {
    let dir = project();
    let out = som(dir.path(), &["build", "main.som", "-v", "--cc", "cc"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cc -std=c11"));
}

#[test]
fn a_directory_program_builds_inside_itself() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("example")).unwrap();
    std::fs::write(dir.path().join("example/main.som"), PROGRAM).unwrap();
    let out = som(dir.path(), &["build", "example", "--emit", "c"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.path().join("example/example").is_file());
    assert!(dir.path().join("example/example.c").is_file());
    // Building again works: the new files don't get in the way.
    assert!(som(dir.path(), &["build", "example"]).status.success());
}

#[test]
fn output_that_is_a_directory_is_reported() {
    let dir = project();
    std::fs::create_dir(dir.path().join("out")).unwrap();
    let out = som(dir.path(), &["build", "main.som", "-o", "out"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("it is a directory"));
}

#[test]
fn a_missing_program_suggests_how_to_create_it() {
    let dir = tempfile::tempdir().unwrap();
    let out = som(dir.path(), &["build", "example"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("create `example.som`, or a directory `example/` with a `main.som` in it"),
        "{stderr}"
    );

    std::fs::write(dir.path().join("example.som"), PROGRAM).unwrap();
    let stderr =
        String::from_utf8_lossy(&som(dir.path(), &["check", "exmple"]).stderr).into_owned();
    assert!(stderr.contains("create `exmple.som`"), "{stderr}");
}

#[test]
fn a_path_missing_its_extension_suggests_the_file() {
    let dir = project();
    let stderr = String::from_utf8_lossy(&som(dir.path(), &["run", "main"]).stderr).into_owned();
    assert!(stderr.contains("did you mean `main.som`?"), "{stderr}");
}

#[test]
fn lsp_accepts_the_stdio_flag_editors_pass() {
    // With stdin closed straight away, the server starts and finds the session over.
    let out = Command::new(env!("CARGO_BIN_EXE_som"))
        .args(["lsp", "--stdio"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn library_path_flag_is_searched() {
    let dir = project();
    std::fs::write(
        dir.path().join("main.som"),
        "extern \"nowhere_som_test\" { fn f(); }\nfn main() {}\n",
    )
    .unwrap();
    let out = som(dir.path(), &["check", "main.som"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("cannot find library `nowhere_som_test`")
    );

    let libs = dir.path().join("libs");
    std::fs::create_dir(&libs).unwrap();
    std::fs::write(libs.join("f.c"), "void f(void) {}\n").unwrap();
    let file = if cfg!(target_os = "macos") {
        "libnowhere_som_test.dylib"
    } else {
        "libnowhere_som_test.so"
    };
    assert!(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-o"])
            .arg(libs.join(file))
            .arg(libs.join("f.c"))
            .status()
            .unwrap()
            .success()
    );
    let out = som(dir.path(), &["check", "main.som", "-L", "libs"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

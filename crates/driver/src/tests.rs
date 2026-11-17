//! Building a program's tests, running them, and reading what the runner
//! (`std/testing.som`) prints: `ok      name` or `FAILED  name (file:line)` for each,
//! after what the test printed itself. A failure's own line is the standard library's
//! `on_fail`'s: `failed at main.som:9:5 in subtracts: off by one`, or `reached @unreachable
//! at main.som:9:5 in pick`.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::{BuildError, BuildOptions, Failed, Options};

/// What `std/testing.som` starts each result line with.
const PASSED: &str = "ok      ";
const FAILED: &str = "FAILED  ";

/// What `on_fail` starts a failure's line with.
const FAILED_AT: &str = "failed at ";
const UNREACHABLE_AT: &str = "reached @unreachable at ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(Failure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// Everything the test printed, its failure's line among it.
    pub printed: String,
    /// Why, as the failure's line says it: `off by one`. All of its last line when that
    /// isn't one, as when its own `@on fail` printed something else.
    pub why: String,
    /// Where, when the failure's line says.
    pub at: Option<FailedAt>,
}

/// A place in the program, as `on_fail` writes it: the file's name, not its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedAt {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone)]
pub struct TestResult {
    /// As the runner names it: `parses_hex`, or `som::parse::parses_hex`.
    pub name: String,
    pub outcome: Outcome,
}

/// Each test that finished, in order.
pub struct TestRun {
    pub results: Vec<TestResult>,
    /// The time ran out: the test after the last result was still running, and the rest
    /// didn't start.
    pub timed_out: bool,
}

#[derive(Debug)]
pub enum TestRunError {
    /// The program has errors.
    Check(Failed),
    Build(BuildError),
    Run(std::io::Error),
}

/// Builds the program at `path` as `som test` does, and runs it for at most `limit`.
pub fn run_tests(path: &Path, options: &Options, limit: Duration) -> Result<TestRun, TestRunError> {
    let options = Options {
        test: true,
        ..options.clone()
    };
    let checked = crate::check(path, &options).map_err(TestRunError::Check)?;
    let dir = scratch_dir();
    std::fs::create_dir_all(&dir).map_err(TestRunError::Run)?;
    let result = (|| {
        let exe = crate::build(&checked, &dir.join("tests"), &BuildOptions::default())
            .map_err(TestRunError::Build)?;
        let (output, timed_out) =
            run_for(crate::command(&checked, &exe, &[]), limit).map_err(TestRunError::Run)?;
        Ok(TestRun {
            results: parse(&output),
            timed_out,
        })
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn scratch_dir() -> PathBuf {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("som-test-{}-{run}", std::process::id()))
}

/// What `command` prints before it exits, or before `limit` runs out, when it's stopped:
/// it and the tests it started.
fn run_for(mut command: std::process::Command, limit: Duration) -> std::io::Result<(String, bool)> {
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    // Read on the side: a test printing more than the pipe holds would otherwise wait on
    // this forever.
    let mut stdout = child.stdout.take().expect("piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let start = Instant::now();
    let timed_out = loop {
        if child.try_wait()?.is_some() {
            break false;
        }
        if start.elapsed() >= limit {
            stop(&mut child);
            break true;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader.join().unwrap_or_default();
    Ok((String::from_utf8_lossy(&bytes).into_owned(), timed_out))
}

/// Stops the runner and the test it forked, which share its process group.
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{}", child.id())])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

/// Each result line, with what its test printed before it.
fn parse(output: &str) -> Vec<TestResult> {
    let mut results = Vec::new();
    let mut printed: Vec<&str> = Vec::new();
    for line in output.lines() {
        if let Some(name) = line.strip_prefix(PASSED) {
            results.push(TestResult {
                name: name.to_string(),
                outcome: Outcome::Passed,
            });
            printed.clear();
        } else if let Some(rest) = line.strip_prefix(FAILED) {
            // `name (file:line)`
            let name = rest.rsplit_once(" (").map_or(rest, |(name, _)| name);
            results.push(TestResult {
                name: name.to_string(),
                outcome: Outcome::Failed(failure(&printed)),
            });
            printed.clear();
        } else {
            printed.push(line);
        }
    }
    results
}

/// What a failed test printed, read for where and why it failed: from the last line saying,
/// or else the last line.
fn failure(printed: &[&str]) -> Failure {
    let said = printed.iter().rev().find_map(|line| failure_line(line));
    let last = printed.iter().rev().find(|l| !l.trim().is_empty());
    let (why, at) = match said {
        Some((why, at)) => (why, Some(at)),
        None => (
            last.map_or_else(String::new, |l| l.trim().to_string()),
            None,
        ),
    };
    Failure {
        printed: printed.join("\n"),
        why,
        at,
    }
}

/// `failed at main.som:9:5 in subtracts: off by one`: why, and where.
fn failure_line(line: &str) -> Option<(String, FailedAt)> {
    let (rest, unreachable) = match line.strip_prefix(FAILED_AT) {
        Some(rest) => (rest, false),
        None => (line.strip_prefix(UNREACHABLE_AT)?, true),
    };
    let (place, after) = rest.split_once(" in ")?;
    let mut parts = place.rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let line_number = parts.next()?.parse().ok()?;
    let file = parts.next()?.to_string();
    let why = match unreachable {
        true => "reached @unreachable".to_string(),
        false => after
            .split_once(": ")
            .map_or(after, |(_, why)| why)
            .to_string(),
    };
    let at = FailedAt {
        file,
        line: line_number,
        column,
    };
    Some((why, at))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn each_result_has_what_its_test_printed() {
        let output = "ok      adds\n\
                      some output\n\
                      failed at main.som:9:5 in subtracts: off by one\n\
                      FAILED  maths::subtracts (main.som:7)\n\
                      reached @unreachable at maths.som:3:9 in pick\n\
                      FAILED  maths::picks (maths.som:2)\n\
                      \n\
                      2 passed, 1 failed\n";
        let results = parse(output);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].name, "adds");
        assert_eq!(results[0].outcome, Outcome::Passed);
        assert_eq!(results[1].name, "maths::subtracts");
        let Outcome::Failed(failed) = &results[1].outcome else {
            panic!("subtracts failed");
        };
        assert_eq!(failed.why, "off by one");
        assert!(failed.printed.starts_with("some output\n"));
        let at = FailedAt {
            file: "main.som".into(),
            line: 9,
            column: 5,
        };
        assert_eq!(failed.at, Some(at));
        let Outcome::Failed(failed) = &results[2].outcome else {
            panic!("picks failed");
        };
        assert_eq!(failed.why, "reached @unreachable");
        assert_eq!(failed.at.as_ref().map(|at| at.line), Some(3));
    }
}

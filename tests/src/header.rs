//! The `// key: value` comment header at the top of a corpus case.
//!
//! ```som
//! // expect: exit 42                    compiles, runs, exits with 42
//! // expect: ok                         passes every check (not run; may have no `main`)
//! // expect: error                      fails with at least one error
//! // expect: error Mismatch MissingImpl fails with at least these error kinds
//! // ignore: reason                     known failure, skipped by default
//! // run: tests                         built as `som test` builds it: its `@test`s, not `main`
//! ```
//!
//! Shared between `build.rs` (to generate tests) and the runner (to check them).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expect {
    Exit(i32),
    Ok,
    Error(Vec<String>),
}

#[derive(Debug, Clone)]
pub struct Header {
    pub expect: Expect,
    pub ignore: Option<String>,
    /// `// run: tests`.
    pub tests: bool,
}

impl Header {
    pub fn parse(source: &str) -> Header {
        let mut expect = None;
        let mut ignore = None;
        let mut tests = false;

        for line in source.lines() {
            let Some(comment) = line.trim().strip_prefix("//") else {
                break;
            };
            let Some((key, value)) = comment.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "expect" => expect = Some(parse_expect(value)),
                "ignore" => ignore = Some(value.to_string()),
                "run" if value == "tests" => tests = true,
                "run" => panic!("unknown `// run:` {value:?}; there's only `tests`"),
                _ => {}
            }
        }

        Header {
            expect: expect.expect("corpus case is missing an `// expect:` header"),
            ignore,
            tests,
        }
    }
}

fn parse_expect(value: &str) -> Expect {
    let mut words = value.split_whitespace();
    match words.next() {
        Some("exit") => Expect::Exit(
            words
                .next()
                .and_then(|code| code.parse().ok())
                .expect("`expect: exit` needs an integer exit code"),
        ),
        Some("ok") => Expect::Ok,
        Some("error") => Expect::Error(words.map(str::to_string).collect()),
        other => panic!("unknown expectation {other:?}"),
    }
}

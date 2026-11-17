//! How the program's tests did when they last ran, shown where they failed: an error on the
//! line that failed (so the scrollbar and the Problems panel have it), and why, at the end
//! of that line.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use som_ast::{Func, Id};
use som_base::Source;
use som_driver::{Analysis, Outcome, TestResult};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, InlayHint, InlayHintLabel,
    InlayHintTooltip, Position, Range,
};

use crate::convert;

/// What the diagnostics say they're from.
const SOURCE: &str = "som test";

/// A program's tests, as they last ran.
#[derive(Clone, Default)]
pub(crate) struct TestStatus {
    /// Running again: the results are of what was saved before.
    pub running: bool,
    pub results: Vec<TestResult>,
    /// They were stopped after this long: the test after the last result didn't finish.
    pub timed_out: Option<std::time::Duration>,
}

/// Where a test failed.
pub(crate) struct Mark {
    pub test: Id<Func>,
    pub name: String,
    /// The file it failed in, by path.
    pub file: String,
    /// The rest of the line that failed, from where; or the test's name, when the failure
    /// didn't say where, or said a file that isn't the program's.
    pub range: Range,
    pub on_line: bool,
    pub why: String,
    pub printed: String,
}

/// Where each failed test of the program failed.
pub(crate) fn failures(a: &Analysis, status: &TestStatus) -> Vec<Mark> {
    let mut marks = Vec::new();
    for test in a.names.program_tests(&a.ast) {
        let name = a.names.test_name(&a.ast, test);
        let Some(Outcome::Failed(failure)) = (status.results.iter())
            .find(|r| r.name == name)
            .map(|r| &r.outcome)
        else {
            continue;
        };
        let test_span = &a.ast.funcs[test].name.span;
        let test_file = test_span.source.identifier();
        let line = failure.at.as_ref().and_then(|at| {
            let source = source_named(a, &at.file, test_file)?;
            let row = at.line.checked_sub(1)? as usize;
            let text = source.content().lines().nth(row)?;
            let start = (at.column.saturating_sub(1) as usize).min(text.len());
            let end = text.trim_end().len().max(start);
            let range = Range {
                start: Position::new(row as u32, utf16_len(&text[..start])),
                end: Position::new(row as u32, utf16_len(&text[..end])),
            };
            Some((source.identifier().to_string(), range))
        });
        let (file, range, on_line) = match line {
            Some((file, range)) => (file, range, true),
            None => (
                test_file.to_string(),
                convert::span_to_range(test_span),
                false,
            ),
        };
        marks.push(Mark {
            test,
            name,
            file,
            range,
            on_line,
            why: failure.why.clone(),
            printed: failure.printed.clone(),
        });
    }
    marks
}

/// An error where each test failed, by file.
pub(crate) fn diagnostics(a: &Analysis, status: &TestStatus) -> HashMap<String, Vec<Diagnostic>> {
    let mut by_file: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for mark in failures(a, status) {
        let related = (mark.on_line)
            .then(|| convert::location(&a.ast.funcs[mark.test].name.span))
            .flatten()
            .map(|location| {
                vec![DiagnosticRelatedInformation {
                    location,
                    message: format!("the test `{}`", mark.name),
                }]
            });
        by_file.entry(mark.file).or_default().push(Diagnostic {
            range: mark.range,
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some(SOURCE.into()),
            message: format!("test `{}` failed: {}", mark.name, mark.why),
            related_information: related,
            ..Default::default()
        });
    }
    by_file
}

/// Why each test that failed in `file` failed, at the end of the line it failed on.
pub(crate) fn hints(a: &Analysis, status: &TestStatus, file: &str) -> Vec<InlayHint> {
    (failures(a, status).into_iter())
        .filter(|mark| mark.on_line && mark.file == file)
        .map(|mark| InlayHint {
            position: mark.range.end,
            label: InlayHintLabel::String(format!("✗ {}", mark.why)),
            kind: None,
            text_edits: None,
            tooltip: (!mark.printed.is_empty())
                .then(|| InlayHintTooltip::String(mark.printed.clone())),
            padding_left: Some(true),
            padding_right: None,
            data: None,
        })
        .collect()
}

/// The program's file named `name` (`main.som`), as a failure says it: `preferred` if
/// that's it, else the only one by that name.
fn source_named(a: &Analysis, name: &str, preferred: &str) -> Option<Arc<Source>> {
    let named = |path: &str| Path::new(path).file_name().is_some_and(|n| n == name);
    let mut found: Vec<Arc<Source>> = Vec::new();
    for (id, _) in a.ast.funcs.iter() {
        let source = &a.ast.funcs.span(id).source;
        if named(source.identifier())
            && !found.iter().any(|s| s.identifier() == source.identifier())
        {
            found.push(source.clone());
        }
    }
    (found.iter().find(|s| s.identifier() == preferred).cloned())
        .or_else(|| (found.len() == 1).then(|| found[0].clone()))
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_support::analyzed;
    use som_driver::{FailedAt, Failure};

    const SOURCE: &str = "@test\nfn adds() {\n    assert(1 + 1 == 2);\n}\n\n\
                          @test\nfn subtracts() {\n    assert(2 - 1 == 0, \"off by one\");  \n}\n\n\
                          fn main() -> i32 = 0;\n";

    fn ran(at: Option<FailedAt>) -> TestStatus {
        let failure = Failure {
            printed: "failed at main.som:8:5 in subtracts: off by one".into(),
            why: "off by one".into(),
            at,
        };
        TestStatus {
            running: false,
            results: vec![
                TestResult {
                    name: "adds".into(),
                    outcome: Outcome::Passed,
                },
                TestResult {
                    name: "subtracts".into(),
                    outcome: Outcome::Failed(failure),
                },
            ],
            timed_out: None,
        }
    }

    #[test]
    fn a_failure_is_an_error_on_the_line_that_failed_and_why_at_its_end() {
        let (_dir, main, a) = analyzed(SOURCE);
        let at = FailedAt {
            file: "main.som".into(),
            line: 8,
            column: 5,
        };
        let status = ran(Some(at));
        let diagnostics = diagnostics(&a, &status);
        let [error] = &diagnostics[&main][..] else {
            panic!("one failure: {diagnostics:?}");
        };
        // From `assert` to the end of the line, trailing spaces left out.
        assert_eq!(error.range.start, Position::new(7, 4));
        assert_eq!(error.range.end, Position::new(7, 37));
        assert_eq!(error.message, "test `subtracts` failed: off by one");
        let hints = hints(&a, &status, &main);
        let [hint] = &hints[..] else {
            panic!("one hint");
        };
        assert_eq!(hint.position, Position::new(7, 37));
        assert!(matches!(&hint.label, InlayHintLabel::String(l) if l == "✗ off by one"));
    }

    #[test]
    fn a_failure_that_doesnt_say_where_is_on_the_test() {
        let (_dir, main, a) = analyzed(SOURCE);
        let status = ran(None);
        let diagnostics = diagnostics(&a, &status);
        let [error] = &diagnostics[&main][..] else {
            panic!("one failure");
        };
        // `subtracts`, on line 7.
        assert_eq!(error.range.start, Position::new(6, 3));
        assert!(hints(&a, &status, &main).is_empty());
    }
}

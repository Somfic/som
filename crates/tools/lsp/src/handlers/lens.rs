use som_ast::{Func, Id, Item, Span};
use som_driver::{Analysis, Outcome};
use som_resolve::DefId;
use tower_lsp::lsp_types::{CodeLens, Command, Range, Url};

use super::tests::TestStatus;
use crate::convert;
use crate::index::items_in_file;
use crate::references::{self, Symbol};

/// The editor extension's commands the lenses run.
const RUN_COMMAND: &str = "som.run";
const SHOW_REFERENCES_COMMAND: &str = "som.showReferences";

/// How many references each item in `file` has, a Run button above the entry point, and
/// how each test did when the tests last ran.
pub(crate) fn code_lenses(
    a: &Analysis,
    file: &str,
    uri: &Url,
    tests: Option<&TestStatus>,
) -> Vec<CodeLens> {
    let mut lenses = reference_lenses(a, file, uri);
    if let Some(tests) = tests {
        lenses.extend(test_lenses(a, file, tests));
    }
    if let Some(main) = entry_point(a, file) {
        let start = convert::span_to_range(a.ast.funcs.span(main)).start;
        lenses.push(CodeLens {
            range: Range { start, end: start },
            command: Some(Command {
                title: "$(play) Run".to_string(),
                command: RUN_COMMAND.to_string(),
                arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
            }),
            data: None,
        });
    }
    lenses
}

/// Above each of the program's tests in `file`: passed, failed and why, or still running.
fn test_lenses(a: &Analysis, file: &str, tests: &TestStatus) -> Vec<CodeLens> {
    let order = a.names.program_tests(&a.ast);
    // Where the run stopped, when it timed out: the test that was running.
    let stuck = tests.timed_out.and_then(|_| {
        (order.iter()).find(|&&t| {
            let name = a.names.test_name(&a.ast, t);
            !tests.results.iter().any(|r| r.name == name)
        })
    });
    (order.iter())
        .filter(|&&t| a.ast.funcs.span(t).source.identifier() == file)
        .filter_map(|&t| {
            let name = a.names.test_name(&a.ast, t);
            let result = tests.results.iter().find(|r| r.name == name);
            let title = match (tests.running, result) {
                (true, _) => "$(sync~spin) running".to_string(),
                (false, Some(r)) => match &r.outcome {
                    Outcome::Passed => "$(pass) passed".to_string(),
                    Outcome::Failed(failure) => format!("$(error) failed: {}", failure.why),
                },
                (false, None) if stuck == Some(&t) => {
                    let after = tests.timed_out?.as_secs();
                    format!("$(watch) still running after {after}s: stopped")
                }
                (false, None) if tests.timed_out.is_some() => "$(circle-slash) not run".into(),
                (false, None) => return None,
            };
            let start = convert::span_to_range(&a.ast.funcs[t].name.span).start;
            Some(CodeLens {
                range: Range { start, end: start },
                // Nothing to do when it's clicked: it's what's known.
                command: Some(Command {
                    title,
                    command: String::new(),
                    arguments: None,
                }),
                data: None,
            })
        })
        .collect()
}

/// `main` in the root module, if `file` has it.
fn entry_point(a: &Analysis, file: &str) -> Option<Id<Func>> {
    items_in_file(&a.ast, file)
        .into_iter()
        .find_map(|item| match item {
            Item::Func(id)
                if &*a.ast.funcs[*id].name.name == som_lang::ENTRY_POINT
                    && a.names.def(a.names.func(*id)).module == 0 =>
            {
                Some(*id)
            }
            _ => None,
        })
}

/// "N references" above each function, type, trait and constant in `file`, and each method
/// in its impl blocks: opening them in a peek.
fn reference_lenses(a: &Analysis, file: &str, uri: &Url) -> Vec<CodeLens> {
    let all = references::occurrences(a);
    let mut names: Vec<(Symbol, Span)> = Vec::new();
    let mut add = |def: DefId, span: &Span| names.push((Symbol::Def(def), span.clone()));
    for item in items_in_file(&a.ast, file) {
        match item {
            Item::Func(f) => add(a.names.func(*f), &a.ast.funcs[*f].name.span),
            Item::Struct(s) => add(a.names.strukt(*s), &a.ast.structs[*s].name.span),
            Item::Trait(t) => add(a.names.trait_def(*t), &a.ast.traits[*t].name.span),
            Item::Const(c) => add(a.names.const_def(*c), &a.ast.consts[*c].name.span),
            Item::Impl(block) => {
                for &m in &block.methods {
                    add(a.names.func(m), &a.ast.funcs[m].name.span);
                }
            }
            Item::Extern(_) | Item::Alias(_) | Item::Use(_) => {}
        }
    }
    (names.into_iter())
        .map(|(symbol, span)| {
            let family = references::family(a, symbol);
            let count = (all.iter())
                .filter(|o| family.contains(&o.symbol) && !o.definition)
                .count();
            let title = match count {
                1 => "1 reference".to_string(),
                n => format!("{n} references"),
            };
            let range = convert::span_to_range(&span);
            CodeLens {
                range,
                command: Some(Command {
                    title,
                    command: SHOW_REFERENCES_COMMAND.to_string(),
                    arguments: Some(vec![
                        serde_json::Value::String(uri.to_string()),
                        range.start.line.into(),
                        range.start.character.into(),
                    ]),
                }),
                data: None,
            }
        })
        .collect()
}

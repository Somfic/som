use super::{Passer, PasserResult};
use crate::parser::ast::Symbol;
use miette::{Result, Severity};

pub struct TypingPasser;

impl Passer for TypingPasser {
    fn pass(ast: &Symbol<'_>) -> Result<PasserResult> {
        let mut critical = vec![];
        let mut non_critical = vec![];

        non_critical.push(miette::miette! {
            severity = Severity::Warning,
            labels = vec![miette::LabeledSpan::at(100, "uwu")],
            "Typing is not yet implemented"
        });

        Ok(PasserResult {
            critical,
            non_critical,
        })
    }
}

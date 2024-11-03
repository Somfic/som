use crate::parser::ast::CombineSpan;
use crate::parser::ast::{Expression, ExpressionValue, Statement, StatementValue};
use miette::{LabeledSpan, SourceSpan};

use super::{walk, Passer, PasserResult};

pub struct UnusedPass;

impl Passer for UnusedPass {
    fn pass(ast: &crate::parser::ast::Symbol<'_>) -> miette::Result<super::PasserResult> {
        walk(
            ast,
            |s: &Statement<'_>| match &s.value {
                StatementValue::Block(statements) => {
                    let mut result = super::PasserResult::default();
                    let mut has_passed_return = false;
                    for statement in statements {
                        if let StatementValue::Return(_) = statement.value {
                            has_passed_return = true;
                            println!("has passed return");
                            continue;
                        }

                        if has_passed_return {
                            println!("unreachable code");

                            result.non_critical.push(miette::miette! {
                                help = "unreachable code",
                                "unreachable code"
                            });
                        }
                    }
                    Ok(result)
                }
                _ => Ok(PasserResult::default()),
            },
            |e: &Expression<'_>| match &e.value {
                ExpressionValue::Block {
                    statements,
                    return_value,
                } => {
                    let mut result = super::PasserResult::default();
                    let mut has_passed_return = false;
                    let mut unreachable_spans = vec![];
                    for statement in statements {
                        if let StatementValue::Return(_) = statement.value {
                            has_passed_return = true;
                            println!("has passed return");
                            continue;
                        }

                        if has_passed_return {
                            unreachable_spans.push(statement.span);
                        }
                    }

                    if has_passed_return {
                        unreachable_spans.push(return_value.span);
                    }

                    if !unreachable_spans.is_empty() {
                        result.non_critical.push(miette::miette! {
                            labels = vec![LabeledSpan::at(SourceSpan::combine(unreachable_spans), "unreachable code")],
                            help = "unreachable code",
                            "unreachable code"
                        });
                    }

                    Ok(result)
                }
                _ => Ok(PasserResult::default()),
            },
        )
    }
}

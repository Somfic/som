use crate::{
    abstract_syntax_tree::Statement,
    concrete_syntax_tree::{grammar::NonTerminal, ConcreteSyntax},
    diagnostic::{Diagnostic, Error, Severity},
};

use super::AstractSyntax;

pub fn build_ast<'a>(
    syntax: &'a ConcreteSyntax<'a>,
) -> Result<AstractSyntax<'a>, Vec<Diagnostic<'a>>> {
    println!("Parsing {}", syntax);

    // match syntax {
    //     ConcreteSyntax::NonTerminal(NonTerminal::EnumDeclaration, children) => {
    //         let identifier = children[1].clone();
    //         let items = children[3].clone();
    //     }
    //     _ => Err(vec![Diagnostic {
    //         severity: Severity::Error,
    //         title: "Failed to build AST".to_string(),
    //         errors: vec![Error::primary(
    //             syntax.range().file_id,
    //             syntax.range().position,
    //             syntax.range().length,
    //             "Failed to build AST",
    //         )],
    //     }]),
    // }

    todo!()
}

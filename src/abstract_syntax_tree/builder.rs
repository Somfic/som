use crate::{
    abstract_syntax_tree::Statement,
    concrete_syntax_tree::{grammar::NonTerminal, ConcreteSyntax},
    diagnostic::{Diagnostic, Error},
};

use super::AstractSyntax;

pub fn build_ast<'a>(
    syntax: &'a ConcreteSyntax<'a>,
) -> Result<AstractSyntax<'a>, Vec<Diagnostic<'a>>> {
    let mut diagnostics = Vec::new();
    let mut ast = AstractSyntax::Statement(Statement::Empty);

    match syntax {
        ConcreteSyntax::NonTerminal(NonTerminal::Start, children) => {
            match children
                .iter()
                .map(|child| build_ast(child))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(_) => {}
                Err(err) => {
                    diagnostics.extend(err);
                }
            }

            ast = AstractSyntax::Statement(Statement::Empty);
        }
        _ => {
            let range = syntax.range();

            diagnostics.push(
                Diagnostic::error("Structure error").with_error(Error::primary(
                    range.file_id,
                    range.position,
                    range.length,
                    format!("Expected start, got {:?}", syntax),
                )),
            );
        }
    }

    if !diagnostics.is_empty() {
        Err(diagnostics)
    } else {
        Ok(ast)
    }
}

pub fn build_top_level_statement<'a>(
    parse_tree: &'a ConcreteSyntax<'a>,
) -> Result<AstractSyntax<'a>, Vec<Diagnostic<'a>>> {
    match parse_tree {
        ConcreteSyntax::NonTerminal(NonTerminal::RootItems, children) => {
            let root_items = children
                .iter()
                .map(|child| build_ast(child))
                .collect::<Result<Vec<_>, _>>()?;

            Ok(AstractSyntax::Statement(Statement::Empty))
        }
        _ => {
            let range = parse_tree.range();

            return Err(vec![Diagnostic::error("Structure error").with_error(
                Error::primary(
                    range.file_id,
                    range.position,
                    range.length,
                    "Expected root items",
                ),
            )]);
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test() {}
}

use core::panic;
use std::collections::HashSet;

use ast::{Statement, Symbol};
use lookup::Lookup;

use crate::scanner::lexeme::{Lexeme, Range, TokenType};

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod macros;
pub mod statement;
pub mod typing;

pub struct Parser<'a> {
    lookup: Lookup,
    lexemes: &'a Vec<Lexeme>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn new(lexemes: &'a Vec<Lexeme>) -> Self {
        Self {
            lookup: Lookup::default(),
            lexemes,
            cursor: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Symbol, HashSet<Diagnostic>> {
        let mut statements = vec![];
        let mut diagnostics = HashSet::new();
        let mut current_error: Option<(Diagnostic, usize)> = None;
        let mut last_safe_cursor = 0;

        while self.cursor < self.lexemes.len() {
            if current_error.is_some() {
                let (error_diagnostic, error_start_cursor) = current_error.take().unwrap();

                // Skip to the next semicolon
                // TODO: This is a naive approach, we should skip to the next statement
                while let Some(Lexeme::Valid(token)) = self.lexemes.get(self.cursor) {
                    self.cursor += 1;

                    // Try to parse the next statement
                    if statement::parse(self, self.cursor).is_ok() {
                        break;
                    };
                }

                // diagnostics.insert(Diagnostic::error(
                //     error_start_cursor,
                //     self.cursor - error_start_cursor,
                //     "Syntax error",
                // ));

                diagnostics.insert(error_diagnostic);
            }

            match statement::parse(self, self.cursor) {
                Ok((statement, new_cursor)) => {
                    self.cursor = new_cursor;
                    last_safe_cursor = new_cursor;
                    statements.push(statement);
                }
                Err(diagnostic) => {
                    self.cursor += 1;
                    current_error = Some((diagnostic, last_safe_cursor));
                }
            };
        }

        if diagnostics.is_empty() {
            Ok(Symbol::Statement(Statement::Block(statements)))
        } else {
            Err(diagnostics)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub range: Range,
    pub message: String,
}

impl Diagnostic {
    fn error(cursor: usize, length: usize, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            range: Range {
                position: cursor,
                length,
            },
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    fn combine(diagnostics: &[Diagnostic]) -> Diagnostic {
        let min_range = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.range.clone())
            .min_by_key(|range| range.position)
            .unwrap();

        let max_range = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.range.clone())
            .max_by_key(|range| range.position + range.length)
            .unwrap();

        Diagnostic::error(
            min_range.position,
            max_range.position + max_range.length - min_range.position,
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<HashSet<_>>()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

#[cfg(test)]
mod tests {
    use ast::{BinaryOperation, Expression};

    use crate::scanner::Scanner;

    use super::*;

    #[test]
    fn parses_addition() {
        let code = "123 + 456;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().expect("Failed to parse");

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Plus,
                    Box::new(Expression::Number(456.0))
                )
            )]))
        );
    }

    #[test]
    fn parses_subtraction() {
        let code = "123 - 456;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let parser = Parser::new(&lexemes).parse();
        let parser = parser.unwrap();

        assert_eq!(
            parser,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Minus,
                    Box::new(Expression::Number(456.0))
                )
            )]))
        );
    }

    #[test]
    fn parses_multiplication() {
        let code = "123 * 456;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().unwrap();

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Times,
                    Box::new(Expression::Number(456.0))
                )
            )]))
        );
    }

    #[test]
    fn parses_division() {
        let code = "123 / 456;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().unwrap();

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Divide,
                    Box::new(Expression::Number(456.0))
                )
            )]))
        );
    }

    #[test]
    fn parses_long_expression() {
        let code = "123 + 456 - 789 + 101;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().unwrap();

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Plus,
                    Box::new(Expression::Binary(
                        Box::new(Expression::Number(456.0)),
                        BinaryOperation::Minus,
                        Box::new(Expression::Binary(
                            Box::new(Expression::Number(789.0)),
                            BinaryOperation::Plus,
                            Box::new(Expression::Number(101.0))
                        ))
                    ))
                )
            )]))
        );
    }

    #[test]
    fn gives_precedence_to_multiplication() {
        let code = "123 * 456 + 789;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().unwrap();

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Binary(
                        Box::new(Expression::Number(123.0)),
                        BinaryOperation::Times,
                        Box::new(Expression::Number(456.0))
                    )),
                    BinaryOperation::Plus,
                    Box::new(Expression::Number(789.0))
                )
            )]))
        );
    }

    #[test]
    fn parses_expression_grouping() {
        let code = "(123 + 456) * 789;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(&lexemes);
        let result = parser.parse().unwrap();

        assert_eq!(
            result,
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Grouping(Box::new(Expression::Binary(
                        Box::new(Expression::Number(123.0)),
                        BinaryOperation::Plus,
                        Box::new(Expression::Number(456.0))
                    )))),
                    BinaryOperation::Times,
                    Box::new(Expression::Number(789.0))
                )
            )]))
        );
    }
}

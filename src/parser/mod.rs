use std::collections::HashSet;

use ast::{Statement, Symbol};
use lookup::Lookup;

use crate::scanner::lexeme::{Lexeme, Range};

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod macros;
pub mod statement;
pub mod typing;

pub struct Parser {
    lookup: Lookup,
    lexemes: Vec<Lexeme>,
    cursor: usize,
}

impl Parser {
    pub fn new(lexemes: Vec<Lexeme>) -> Self {
        Self {
            lookup: Lookup::default(),
            lexemes,
            cursor: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Symbol, HashSet<Diagnostic>> {
        let mut statements = vec![];
        let mut diagnostics = HashSet::new();

        for result in self.into_iter() {
            match result {
                Ok(statement) => statements.push(statement),
                Err(diagnostic) => {
                    diagnostics.insert(diagnostic);
                }
            }
        }

        if diagnostics.is_empty() {
            Ok(Symbol::Statement(Statement::Block(statements)))
        } else {
            Err(diagnostics)
        }
    }
}

impl Iterator for Parser {
    type Item = Result<Statement, Diagnostic>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.lexemes.len() {
            return None;
        }

        match statement::parse(self, self.cursor) {
            Ok((statement, new_cursor)) => {
                self.cursor = new_cursor;
                Some(Ok(statement))
            }
            Err(diagnostic) => {
                self.cursor += 1;
                // TODO: Diagnostic::combine here?
                Some(Err(diagnostic))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub range: Range,
    pub message: String,
    pub context: Option<Box<Diagnostic>>,
}

impl Diagnostic {
    fn error(range: &Range, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            range: range.clone(),
            message: message.into(),
            context: None,
        }
    }

    #[allow(dead_code)]
    fn combine(diagnostics: Vec<Diagnostic>) -> Diagnostic {
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
            &Range {
                position: min_range.position,
                length: max_range.position + max_range.length - min_range.position,
            },
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<HashSet<_>>()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
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
        let mut parser = Parser::new(lexemes);
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
        let parser = Parser::new(lexemes).parse();
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
        let mut parser = Parser::new(lexemes);
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
        let mut parser = Parser::new(lexemes);
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
        let mut parser = Parser::new(lexemes);
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
        let mut parser = Parser::new(lexemes);
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
        let mut parser = Parser::new(lexemes);
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

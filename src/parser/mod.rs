use crate::{
    diagnostic::{Diagnostic, Error},
    scanner::lexeme::Lexeme,
};
use ast::{Statement, Symbol};
use lookup::Lookup;
use std::collections::HashSet;

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod macros;
pub mod statement;
pub mod typing;

pub struct Parser<'a> {
    lookup: Lookup<'a>,
    lexemes: &'a Vec<Lexeme<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(lexemes: &'a Vec<Lexeme<'a>>) -> Self {
        Self {
            lookup: Lookup::default(),
            lexemes,
        }
    }

    pub fn parse(&'a mut self) -> Result<Symbol, HashSet<Diagnostic>> {
        let mut statements = vec![];
        let mut diagnostics = HashSet::new();
        let mut last_safe_cursor = 0;
        let mut cursor = 0;
        let mut panic_mode = false;

        while cursor < self.lexemes.len() {
            if panic_mode {
                // Skip to the next valid statement
                while let Some(lexeme) = self.lexemes.get(cursor) {
                    // Try to parse the next statement
                    if lexeme.is_valid() && statement::parse(self, cursor).is_ok() {
                        diagnostics.insert(
                            Diagnostic::warning("Unparsed code").with_error(
                                Error::primary(
                                    lexeme.range().file_id,
                                    cursor,
                                    cursor - last_safe_cursor,
                                    "This code was not parsed",
                                )
                                .transform_range(self.lexemes),
                            ),
                        );

                        break;
                    };

                    cursor += 1;
                }
            }

            match statement::parse(self, cursor) {
                Ok((statement, new_cursor)) => {
                    cursor = new_cursor;
                    last_safe_cursor = new_cursor;
                    statements.push(statement);
                }
                Err(error) => {
                    diagnostics.insert(
                        Diagnostic::error("Syntax error")
                            .with_error(error.clone().transform_range(self.lexemes)),
                    );
                    panic_mode = true;
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

#[cfg(test)]
mod tests {
    use ast::{BinaryOperation, Expression};

    use crate::{files::Files, scanner::Scanner};

    use super::*;

    #[test]
    fn parses_addition() {
        test_parser(
            "123 + 456;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Plus,
                    Box::new(Expression::Number(456.0)),
                ),
            )])),
        );
    }

    #[test]
    fn parses_subtraction() {
        test_parser(
            "123 - 456;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Minus,
                    Box::new(Expression::Number(456.0)),
                ),
            )])),
        );
    }

    #[test]
    fn parses_multiplication() {
        test_parser(
            "123 * 456;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Times,
                    Box::new(Expression::Number(456.0)),
                ),
            )])),
        );
    }

    #[test]
    fn parses_division() {
        test_parser(
            "123 / 456;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Number(123.0)),
                    BinaryOperation::Divide,
                    Box::new(Expression::Number(456.0)),
                ),
            )])),
        );
    }

    #[test]
    fn parses_long_expression() {
        test_parser(
            "123 + 456 - 789 + 101;",
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
                            Box::new(Expression::Number(101.0)),
                        )),
                    )),
                ),
            )])),
        );
    }

    #[test]
    fn gives_precedence_to_multiplication() {
        test_parser(
            "123 * 456 + 789;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Binary(
                        Box::new(Expression::Number(123.0)),
                        BinaryOperation::Times,
                        Box::new(Expression::Number(456.0)),
                    )),
                    BinaryOperation::Plus,
                    Box::new(Expression::Number(789.0)),
                ),
            )])),
        );
    }

    #[test]
    fn parses_expression_grouping() {
        test_parser(
            "(123 + 456) * 789;",
            Symbol::Statement(Statement::Block(vec![Statement::Expression(
                Expression::Binary(
                    Box::new(Expression::Grouping(Box::new(Expression::Binary(
                        Box::new(Expression::Number(123.0)),
                        BinaryOperation::Plus,
                        Box::new(Expression::Number(456.0)),
                    )))),
                    BinaryOperation::Times,
                    Box::new(Expression::Number(789.0)),
                ),
            )])),
        );
    }

    fn test_parser(code: &str, expected: Symbol) {
        let mut files = Files::default();
        files.insert("test", code);

        let scanner = Scanner::new(&files);
        let lexemes = scanner.parse();

        let mut parser = Parser::new(&lexemes);
        let parsed = parser.parse().unwrap();

        assert_eq!(parsed, expected);
    }
}

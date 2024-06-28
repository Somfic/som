use crate::{
    diagnostic::{Diagnostic, Error},
    scanner::lexeme::Token,
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
    tokens: &'a [Token<'a>],
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a Vec<Token<'a>>) -> Self {
        Self {
            lookup: Lookup::default(),
            tokens,
        }
    }

    pub fn parse(&'a mut self) -> (Symbol, HashSet<Diagnostic>) {
        let mut statements = vec![];
        let mut diagnostics = HashSet::new();
        let mut last_safe_cursor = 0;
        let mut cursor = 0;
        let mut panic_mode = false;

        while cursor < self.tokens.len() {
            if panic_mode {
                // Skip to the next valid statement
                while let Some(token) = self.tokens.get(cursor) {
                    // Try to parse the next statement
                    if statement::parse(self, cursor).is_ok() {
                        // diagnostics.insert(
                        //     Diagnostic::warning("Unparsed code").with_error(
                        //         Error::primary(
                        //             token.range.file_id,
                        //             cursor,
                        //             cursor - last_safe_cursor,
                        //             "This code was not parsed",
                        //         )
                        //         .with_note("This code was not parsed since it ")
                        //         .transform_range(self.tokens),
                        //     ),
                        // );

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
                    let mut diagnostic = Diagnostic::error("Syntax error");

                    for error in error {
                        diagnostic =
                            diagnostic.with_error(error.clone().transform_range(self.tokens));
                    }

                    diagnostics.insert(diagnostic);
                    panic_mode = true;
                }
            };
        }

        (Symbol::Statement(Statement::Block(statements)), diagnostics)
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
        let tokens = scanner.parse().0;

        let mut parser = Parser::new(&tokens);
        let parsed = parser.parse();

        assert_eq!(parsed.0, expected);
    }
}

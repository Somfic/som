use crate::{diagnostic::Diagnostic, files::Files, scanner::lexeme::Token};
use ast::{Satement, StatementSymbol, Symbol};
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use lookup::Lookup;
use std::collections::HashSet;

pub mod ast;
pub mod expression;
pub mod lookup;
pub mod macros;
pub mod statement;
pub mod typest;

pub type ParseResult<'a, T> = Result<T, Diagnostic<'a>>;

pub struct Parser<'a> {
    tokens: &'a [Token<'a>],
    cursor: usize,
    lookup: lookup::Lookup<'a>,
    diagnostics: HashSet<Diagnostic<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token<'a>]) -> Self {
        Self {
            tokens,
            cursor: 0,
            lookup: Lookup::default(),
            diagnostics: HashSet::new(),
        }
    }

    pub fn parse(&mut self) -> ParseResult<'a, StatementSymbol> {
        let mut statements = Vec::new();
        let mut panic_mode = false;

        while self.has_tokens() {
            match statement::parse(self) {
                Ok(statement) => {
                    if panic_mode {
                        panic_mode = false;
                    }

                    statements.push(statement);
                }
                Err(diagnostic) => {
                    if !panic_mode {
                        self.diagnostics.insert(diagnostic);
                    }

                    self.consume();
                    panic_mode = true;
                }
            }
        }

        Ok(StatementSymbol::new(Satement::Block(statements)))
    }

    pub(crate) fn peek(&self) -> Option<&Token<'a>> {
        self.tokens.get(self.cursor)
    }

    pub(crate) fn consume(&mut self) -> Option<&Token<'a>> {
        self.cursor += 1;
        self.tokens.get(self.cursor - 1)
    }

    pub(crate) fn has_tokens(&self) -> bool {
        self.cursor < self.tokens.len()
    }

    pub fn print_diagnostics(&self, files: &Files) {
        let diagnostics: Vec<codespan_reporting::diagnostic::Diagnostic<&str>> =
            self.diagnostics.iter().map(|d| d.clone().into()).collect();

        let writer = StandardStream::stderr(ColorChoice::Auto);
        let config = codespan_reporting::term::Config::default();

        for diagnostic in diagnostics {
            codespan_reporting::term::emit(&mut writer.lock(), &config, files, &diagnostic)
                .unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use ast::{BinaryOperation, Expression, Symbol};

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
        let scanner_pass = scanner.parse();

        let mut parser = Parser::new(&scanner_pass.result);
        let parse_pass = parser.parse();

        parser.print_diagnostics(&files);

        match &parse_pass {
            Ok(parsed) => {
                assert_eq!(parsed, &expected);
            }
            Err(_) => {
                panic!("Parser failed to parse the code");
            }
        }
    }
}

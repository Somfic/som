use lookup::Lookup;

use crate::scanner::lexeme::{Lexeme, Token};

pub mod expression;
pub mod lookup;
pub mod statement;

#[derive(Debug, Clone)]
pub enum Symbol {
    Terminal(Token),
    NonTerminal(NonTerminal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum NonTerminal {
    Expression(Expression),
    Statement(Statement),
    Unknown(Lexeme),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Number(f64),
    String(String),
    Symbol(String),
    Binary(Box<Expression>, BinaryOperation, Box<Expression>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Block(Vec<Statement>),
    Expression(Expression),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOperation {
    Plus,
    Minus,
    Times,
    Divide,
}

pub struct Parser {
    lookup: Lookup,
    lexemes: Vec<Lexeme>,
}

impl Parser {
    pub fn new(lexemes: Vec<Lexeme>) -> Self {
        Self {
            lookup: Lookup::default(),
            lexemes,
        }
    }

    pub fn parse(&mut self) -> NonTerminal {
        let mut statements = vec![];
        let mut cursor = 0;

        while cursor < self.lexemes.len() {
            match statement::parse(self, cursor) {
                Some((statement, new_cursor)) => {
                    cursor = new_cursor;
                    statements.push(statement);
                }
                None => panic!("Failed to parse statement"),
            }
        }

        NonTerminal::Statement(Statement::Block(statements))
    }
}

#[cfg(test)]
mod tests {
    use crate::scanner::Scanner;

    use super::*;

    #[test]
    fn parses_addition() {
        let code = "123 + 456;";
        let lexemes = Scanner::new(code.to_owned()).collect::<Vec<_>>();
        let mut parser = Parser::new(lexemes);
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
        let mut parser = Parser::new(lexemes);
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
        let result = parser.parse();

        assert_eq!(
            result,
            NonTerminal::Statement(Statement::Block(vec![Statement::Expression(
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
}

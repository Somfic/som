use std::collections::HashMap;

use super::{expression, BinaryOperation, Expression, Parser, Statement};
use crate::scanner::lexeme::{Lexeme, Token};

macro_rules! expect_tokens {
     ($parser:expr, $cursor:expr, $(($($token:pat),*)),*) => {{
            let mut i = $cursor;
            let mut lexemes = Vec::new();
             $(
                let lexeme = $parser.lexemes.get(i)?;

                if let Lexeme::Valid(token, _) = lexeme {
                    let mut matched = false;
                     $(
                        if let $token = token {
                            matched = true;
                        }
                    )*
                    // Check if the token matches any of the patterns in the tuple
                    if matched {
                        lexemes.push(lexeme);
                        i += 1;
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            )*

            // If all tokens matched, return the matched tokens
            Some((lexemes, i))
        }};
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum BindingPower {
    None = 0,
    Comma = 1,
    Assignment = 2,
    Logical = 3,
    Relational = 4,
    Additive = 5,
    Multiplicative = 6,
    Unary = 7,
    Call = 8,
    Member = 9,
    Primary = 10,
}

//
pub type StatementHandler = fn(&Parser, usize) -> Option<(Statement, usize)>;
pub type ExpressionHandler = fn(&Parser, usize) -> Option<(Expression, usize)>;
pub type LeftExpressionHandler =
    fn(&Parser, usize, Expression, &BindingPower) -> Option<(Expression, usize)>;

pub struct Lookup {
    pub statement_lookup: HashMap<Token, StatementHandler>,
    pub expression_lookup: HashMap<Token, ExpressionHandler>,
    pub left_expression_lookup: HashMap<Token, LeftExpressionHandler>,
    pub binding_power_lookup: HashMap<Token, BindingPower>,
}

impl Lookup {
    pub(crate) fn add_statement_handler(&mut self, token: Token, handler: StatementHandler) {
        self.statement_lookup.insert(token, handler);
    }

    pub(crate) fn add_expression_handler(&mut self, token: Token, handler: ExpressionHandler) {
        self.expression_lookup.insert(token, handler);
    }

    pub(crate) fn add_left_expression_handler(
        &mut self,
        token: Token,
        binding_power: BindingPower,
        handler: LeftExpressionHandler,
    ) {
        self.left_expression_lookup.insert(token.clone(), handler);
        self.binding_power_lookup.insert(token, binding_power);
    }
}

impl Default for Lookup {
    fn default() -> Self {
        let mut lookup = Lookup {
            statement_lookup: HashMap::new(),
            expression_lookup: HashMap::new(),
            left_expression_lookup: HashMap::new(),
            binding_power_lookup: HashMap::new(),
        };

        // Addative
        lookup.add_left_expression_handler(
            Token::Plus,
            BindingPower::Additive,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Additive)?;
                Some((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Plus, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        lookup.add_left_expression_handler(
            Token::Minus,
            BindingPower::Additive,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Additive)?;
                Some((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Minus, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        // Multiplicative
        lookup.add_left_expression_handler(
            Token::Star,
            BindingPower::Multiplicative,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Multiplicative)?;
                Some((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Times, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        lookup.add_left_expression_handler(
            Token::Slash,
            BindingPower::Multiplicative,
            |parser, cursor, lhs, _binding| {
                let (rhs, cursor) =
                    super::expression::parse(parser, cursor + 1, &BindingPower::Multiplicative)?;
                Some((
                    Expression::Binary(Box::new(lhs), BinaryOperation::Divide, Box::new(rhs)),
                    cursor,
                ))
            },
        );

        // Literals and symbols
        lookup.add_expression_handler(Token::Decimal(0.0), |parser, cursor| {
            let (tokens, cursor) = expect_tokens!(parser, cursor, (Token::Decimal(_)))?;
            let integer = tokens.first().unwrap();
            if let Lexeme::Valid(Token::Decimal(value), _) = integer {
                Some((Expression::Number(*value), cursor))
            } else {
                None
            }
        });

        lookup.add_expression_handler(Token::Integer(0), |parser, cursor| {
            let (tokens, cursor) = expect_tokens!(parser, cursor, (Token::Integer(_)))?;
            let integer = tokens.first().unwrap();
            if let Lexeme::Valid(Token::Integer(value), _) = integer {
                Some((Expression::Number(*value as f64), cursor))
            } else {
                None
            }
        });

        lookup.add_expression_handler(Token::String("".to_string()), |parser, cursor| {
            let (tokens, cursor) = expect_tokens!(parser, cursor, (Token::String(_)))?;
            let string = tokens.first().unwrap();
            if let Lexeme::Valid(Token::String(value), _) = string {
                Some((Expression::String(value.clone()), cursor))
            } else {
                None
            }
        });

        lookup.add_expression_handler(Token::Identifier("".to_string()), |parser, cursor| {
            let (tokens, cursor) = expect_tokens!(parser, cursor, (Token::Identifier(_)))?;
            let identifier = tokens.first().unwrap();
            if let Lexeme::Valid(Token::Identifier(value), _) = identifier {
                Some((Expression::Symbol(value.clone()), cursor))
            } else {
                None
            }
        });

        lookup.add_statement_handler(Token::Semicolon, |parser, cursor| {
            let (expression, cursor) = expression::parse(parser, cursor, &BindingPower::Primary)?;
            Some((Statement::Expression(expression), cursor))
        });

        lookup
    }
}

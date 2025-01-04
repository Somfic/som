pub mod token;
pub use token::*;

use miette::{LabeledSpan, Result, SourceSpan};

pub struct Lexer<'de> {
    whole: &'de str,
    remainder: &'de str,
    byte_offset: usize,
    peeked: Option<Result<Token<'de>, miette::Error>>,
}

impl<'de> Lexer<'de> {
    pub fn new(input: &'de str) -> Self {
        Self {
            whole: input,
            remainder: input,
            byte_offset: 0,
            peeked: None,
        }
    }

    pub fn expect(
        &mut self,
        expected: TokenKind,
        unexpected: &str,
    ) -> Result<Token<'de>, miette::Error> {
        match self.next() {
            Some(Ok(token)) if expected == token.kind => Ok(token),
            Some(Ok(token)) => Err(miette::miette! {
                labels = vec![
                    token.label(format!("expected {} here", expected))
                ],
                help = format!("expected {}, got {} instead", expected, token.kind),
                "{unexpected}",
            }
          ),
            Some(Err(e)) => Err(e),
            None => Err(miette::miette! {
                labels = vec![
                    LabeledSpan::at_offset(self.byte_offset - 1, format!("Expected {} here", expected))
                ],
                help = format!("{} was expected, but no more code was found", expected),
                "unexpected end of input",
            }
            .wrap_err(unexpected.to_string())
            ),
        }
    }

    pub fn expect_where(
        &mut self,
        mut check: impl FnMut(&Token<'de>) -> bool,
        unexpected: &str,
    ) -> Result<Token<'de>, miette::Error> {
        match self.next() {
            Some(Ok(token)) if check(&token) => Ok(token),
            Some(Ok(token)) => Err(miette::miette! {
                labels = vec![
                    token.label("here")
                ],
                help = format!("expected {token:?}"),
                "{unexpected}",
            }),
            Some(Err(e)) => Err(e),
            None => Err(miette::miette! {
                labels = vec![
                    LabeledSpan::at_offset(self.byte_offset - 1, "expected more source code here")
                ],
                help = "more source code was expected, but none was found",
                "{unexpected}",
            }),
        }
    }

    pub fn expect_any(&mut self, unexpected: &str) -> Result<Token<'de>, miette::Error> {
        self.expect_where(|_| true, unexpected)
    }

    pub fn peek(&mut self) -> Option<&Result<Token<'de>, miette::Error>> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        self.peeked = self.next();
        self.peeked.as_ref()
    }

    pub fn peek_expect(
        &mut self,
        expected: TokenKind,
    ) -> Option<&Result<Token<'de>, miette::Error>> {
        match self.peek() {
            Some(Ok(token::Token { kind, .. })) => {
                if *kind == expected {
                    self.peeked.as_ref()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_compound_operator(
        &mut self,
        single: TokenKind,
        compound: TokenKind,
        expected_char: char,
    ) -> Result<(TokenKind, TokenValue<'de>)> {
        if let Some(c) = self.remainder.chars().next() {
            if c == expected_char {
                self.remainder = &self.remainder[c.len_utf8()..];
                self.byte_offset += c.len_utf8();
                Ok((compound, TokenValue::None))
            } else {
                Ok((single, TokenValue::None))
            }
        } else {
            Ok((single, TokenValue::None))
        }
    }
}

impl<'de> Iterator for Lexer<'de> {
    type Item = Result<Token<'de>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.peeked.take() {
            return Some(next);
        }

        let mut chars = self.remainder.chars();

        let start_offset = self.byte_offset;

        let c = chars.next()?;
        self.remainder = chars.as_str();
        self.byte_offset += c.len_utf8();

        let kind: Result<(TokenKind, TokenValue<'de>)> = match c {
            '(' => Ok((TokenKind::ParenOpen, TokenValue::None)),
            ')' => Ok((TokenKind::ParenClose, TokenValue::None)),
            '{' => Ok((TokenKind::CurlyOpen, TokenValue::None)),
            '}' => Ok((TokenKind::CurlyClose, TokenValue::None)),
            '[' => Ok((TokenKind::SquareOpen, TokenValue::None)),
            ']' => Ok((TokenKind::SquareClose, TokenValue::None)),
            ';' => Ok((TokenKind::Semicolon, TokenValue::None)),
            ',' => Ok((TokenKind::Comma, TokenValue::None)),
            '.' => Ok((TokenKind::Dot, TokenValue::None)),
            '@' => Ok((TokenKind::At, TokenValue::None)),
            '#' => Ok((TokenKind::Hash, TokenValue::None)),
            '$' => Ok((TokenKind::Dollar, TokenValue::None)),
            '|' => Ok((TokenKind::Pipe, TokenValue::None)),
            '^' => Ok((TokenKind::Caret, TokenValue::None)),
            '~' => Ok((TokenKind::Tilde, TokenValue::None)),
            '?' => Ok((TokenKind::Question, TokenValue::None)),
            ':' => Ok((TokenKind::Colon, TokenValue::None)),
            '-' => self.parse_compound_operator(TokenKind::Minus, TokenKind::Arrow, '>'),
            '+' => Ok((TokenKind::Plus, TokenValue::None)),
            '*' => Ok((TokenKind::Star, TokenValue::None)),
            '/' => Ok((TokenKind::Slash, TokenValue::None)),
            '%' => Ok((TokenKind::Percent, TokenValue::None)),
            '=' => self.parse_compound_operator(TokenKind::Equal, TokenKind::Equality, '='),
            '!' => self.parse_compound_operator(TokenKind::Not, TokenKind::Inequality, '='),
            '<' => {
                self.parse_compound_operator(TokenKind::LessThan, TokenKind::LessThanOrEqual, '=')
            }
            '>' => self.parse_compound_operator(
                TokenKind::GreaterThan,
                TokenKind::GreaterThanOrEqual,
                '=',
            ),
            'a'..='z' | 'A'..='Z' | '_' => {
                // Identifiers
                let mut ident = String::new();
                ident.push(c);
                while let Some(c) = self.remainder.chars().next() {
                    if c.is_alphanumeric() || c == '_' {
                        ident.push(c);
                        self.remainder = &self.remainder[c.len_utf8()..];
                        self.byte_offset += c.len_utf8();
                    } else {
                        break;
                    }
                }

                match ident.as_str() {
                    "if" => Ok((TokenKind::If, TokenValue::None)),
                    "else" => Ok((TokenKind::Else, TokenValue::None)),
                    "fn" => Ok((TokenKind::Function, TokenValue::None)),
                    "true" => Ok((TokenKind::Boolean, TokenValue::Boolean(true))),
                    "false" => Ok((TokenKind::Boolean, TokenValue::Boolean(false))),
                    "let" => Ok((TokenKind::Let, TokenValue::None)),
                    "type" => Ok((TokenKind::Type, TokenValue::None)),
                    "struct" => Ok((TokenKind::Struct, TokenValue::None)),
                    "enum" => Ok((TokenKind::Enum, TokenValue::None)),
                    "trait" => Ok((TokenKind::Trait, TokenValue::None)),
                    "bool" => Ok((TokenKind::BooleanType, TokenValue::None)),
                    "int" => Ok((TokenKind::IntegerType, TokenValue::None)),
                    "dec" => Ok((TokenKind::DecimalType, TokenValue::None)),
                    "str" => Ok((TokenKind::StringType, TokenValue::None)),
                    "char" => Ok((TokenKind::CharacterType, TokenValue::None)),
                    "return" => Ok((TokenKind::Return, TokenValue::None)),
                    ident => Ok((
                        TokenKind::Identifier,
                        TokenValue::Identifier(ident.to_string().into()),
                    )),
                }
            }
            // Whole and decimal numbers
            '0'..='9' => {
                let mut number = String::new();
                number.push(c);
                while let Some(c) = self.remainder.chars().next() {
                    if c.is_ascii_digit() || c == '.' {
                        number.push(c);
                        self.remainder = &self.remainder[c.len_utf8()..];
                        self.byte_offset += c.len_utf8();
                    } else {
                        break;
                    }
                }

                if let Ok(num) = number.parse::<i64>() {
                    Ok((TokenKind::Integer, TokenValue::Integer(num)))
                } else if let Ok(num) = number.parse::<f64>() {
                    Ok((TokenKind::Decimal, TokenValue::Decimal(num)))
                } else {
                    Err(miette::miette! {
                        labels = vec![
                            LabeledSpan::at(self.byte_offset - number.len()..self.byte_offset, "this number")
                        ],
                        "invalid number"
                    })
                }
            }
            '"' => {
                let mut string = String::new();
                while let Some(c) = self.remainder.chars().next() {
                    if c == '"' {
                        self.remainder = &self.remainder[c.len_utf8()..];
                        self.byte_offset += c.len_utf8();
                        break;
                    } else {
                        string.push(c);
                        self.remainder = &self.remainder[c.len_utf8()..];
                        self.byte_offset += c.len_utf8();
                    }
                }

                Ok((TokenKind::String, TokenValue::String(string.into())))
            }
            '\'' => {
                let c = self.remainder.chars().next()?;
                self.remainder = &self.remainder[c.len_utf8()..];
                self.byte_offset += c.len_utf8();

                if self.remainder.chars().next()? == '\'' {
                    self.remainder = &self.remainder[c.len_utf8()..];
                    self.byte_offset += c.len_utf8();
                    Ok((TokenKind::Character, TokenValue::Character(c)))
                } else {
                    Err(miette::miette! {
                        labels = vec![
                            LabeledSpan::at(self.byte_offset..self.byte_offset + c.len_utf8(), "this character")
                        ],
                        "expected closing single quote"
                    })
                }
            }
            ' ' | '\r' | '\t' | '\n' => {
                return self.next();
            }
            _ => Err(miette::miette! {
                labels = vec![
                    LabeledSpan::at(self.byte_offset - c.len_utf8()..self.byte_offset, "this character")
                ],
                "unexpected character '{c}' in input"
            }),
        };

        let byte_length = self
            .byte_offset
            .checked_sub(start_offset)
            .expect("byte_offset should never be less than start_offset");

        Some(kind.map(|(kind, value)| Token {
            kind,
            value,
            span: SourceSpan::new(start_offset.into(), byte_length),
            original: &self.whole[start_offset..self.byte_offset],
        }))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn punctuation() {
        test_tokens_eq(
            Lexer::new(".,@#$~?:-|+*/^% () [] {}"),
            vec![
                (TokenKind::Dot, TokenValue::None),
                (TokenKind::Comma, TokenValue::None),
                (TokenKind::At, TokenValue::None),
                (TokenKind::Hash, TokenValue::None),
                (TokenKind::Dollar, TokenValue::None),
                (TokenKind::Tilde, TokenValue::None),
                (TokenKind::Question, TokenValue::None),
                (TokenKind::Colon, TokenValue::None),
                (TokenKind::Minus, TokenValue::None),
                (TokenKind::Pipe, TokenValue::None),
                (TokenKind::Plus, TokenValue::None),
                (TokenKind::Star, TokenValue::None),
                (TokenKind::Slash, TokenValue::None),
                (TokenKind::Caret, TokenValue::None),
                (TokenKind::Percent, TokenValue::None),
                (TokenKind::ParenOpen, TokenValue::None),
                (TokenKind::ParenClose, TokenValue::None),
                (TokenKind::SquareOpen, TokenValue::None),
                (TokenKind::SquareClose, TokenValue::None),
                (TokenKind::CurlyOpen, TokenValue::None),
                (TokenKind::CurlyClose, TokenValue::None),
            ],
        );
    }

    #[test]
    fn comparison() {
        test_tokens_eq(
            Lexer::new("= != == < > <= >="),
            vec![
                (TokenKind::Equal, TokenValue::None),
                (TokenKind::Inequality, TokenValue::None),
                (TokenKind::Equality, TokenValue::None),
                (TokenKind::LessThan, TokenValue::None),
                (TokenKind::GreaterThan, TokenValue::None),
                (TokenKind::LessThanOrEqual, TokenValue::None),
                (TokenKind::GreaterThanOrEqual, TokenValue::None),
            ],
        );
    }

    #[test]
    fn keywords() {
        test_tokens_eq(
            Lexer::new("if else"),
            vec![
                (TokenKind::If, TokenValue::None),
                (TokenKind::Else, TokenValue::None),
            ],
        );
    }

    #[test]
    fn numbers() {
        test_tokens_eq(
            Lexer::new("1 1.0 0.1"),
            vec![
                (TokenKind::Integer, TokenValue::Integer(1)),
                (TokenKind::Decimal, TokenValue::Decimal(1.0)),
                (TokenKind::Decimal, TokenValue::Decimal(0.1)),
            ],
        );
    }

    #[test]
    fn strings() {
        test_tokens_eq(
            Lexer::new("\"foo\" \"bar\" \"baz\""),
            vec![
                (TokenKind::String, TokenValue::String("foo".into())),
                (TokenKind::String, TokenValue::String("bar".into())),
                (TokenKind::String, TokenValue::String("baz".into())),
            ],
        );
    }

    #[test]
    fn characters() {
        test_tokens_eq(
            Lexer::new("'a' 'b' 'c'"),
            vec![
                (TokenKind::Character, TokenValue::Character('a')),
                (TokenKind::Character, TokenValue::Character('b')),
                (TokenKind::Character, TokenValue::Character('c')),
            ],
        );
    }

    #[test]
    fn booleans() {
        test_tokens_eq(
            Lexer::new("true false"),
            vec![
                (TokenKind::Boolean, TokenValue::Boolean(true)),
                (TokenKind::Boolean, TokenValue::Boolean(false)),
            ],
        );
    }

    #[test]
    fn identifiers() {
        test_tokens_eq(
            Lexer::new("foo bar baz"),
            vec![
                (TokenKind::Identifier, TokenValue::Identifier("foo".into())),
                (TokenKind::Identifier, TokenValue::Identifier("bar".into())),
                (TokenKind::Identifier, TokenValue::Identifier("baz".into())),
            ],
        );
    }

    #[test]
    fn program() {
        let program = "
            fn main(self) ~ number {
                print(\"{self.name} ({self.age}) is purring\");
            };
        ";

        let lexer = Lexer::new(program);
        let expected_tokens = vec![
            (TokenKind::Function, TokenValue::None),
            (TokenKind::Identifier, TokenValue::Identifier("main".into())),
            (TokenKind::ParenOpen, TokenValue::None),
            (TokenKind::Identifier, TokenValue::Identifier("self".into())),
            (TokenKind::ParenClose, TokenValue::None),
            (TokenKind::Tilde, TokenValue::None),
            (
                TokenKind::Identifier,
                TokenValue::Identifier("number".into()),
            ),
            (TokenKind::CurlyOpen, TokenValue::None),
            (
                TokenKind::Identifier,
                TokenValue::Identifier("print".into()),
            ),
            (TokenKind::ParenOpen, TokenValue::None),
            (
                TokenKind::String,
                TokenValue::String("{self.name} ({self.age}) is purring".into()),
            ),
            (TokenKind::ParenClose, TokenValue::None),
            (TokenKind::Semicolon, TokenValue::None),
            (TokenKind::CurlyClose, TokenValue::None),
            (TokenKind::Semicolon, TokenValue::None),
        ];

        test_tokens_eq(lexer, expected_tokens);
    }

    #[test]
    fn peeking() {
        let input = "1 2 3";

        let mut lexer = Lexer::new(input);

        let first = lexer.peek().unwrap().as_ref().unwrap();
        assert_eq!(first.kind, TokenKind::Integer);
        assert_eq!(first.value, TokenValue::Integer(1));

        let first = lexer.next().unwrap().unwrap();
        assert_eq!(first.kind, TokenKind::Integer);
        assert_eq!(first.value, TokenValue::Integer(1));

        let second = lexer.peek().unwrap().as_ref().unwrap();
        assert_eq!(second.kind, TokenKind::Integer);
        assert_eq!(second.value, TokenValue::Integer(2));

        let second = lexer.next().unwrap().unwrap();
        assert_eq!(second.kind, TokenKind::Integer);
        assert_eq!(second.value, TokenValue::Integer(2));

        let third = lexer.peek().unwrap().as_ref().unwrap();
        assert_eq!(third.kind, TokenKind::Integer);
        assert_eq!(third.value, TokenValue::Integer(3));

        let third = lexer.next().unwrap().unwrap();
        assert_eq!(third.kind, TokenKind::Integer);
        assert_eq!(third.value, TokenValue::Integer(3));
    }

    fn test_tokens_eq(lexer: Lexer<'_>, tokens: Vec<(TokenKind, TokenValue<'_>)>) {
        let actual_tokens = lexer
            .map(Result::unwrap)
            .map(|x| (x.kind, x.value))
            .collect::<Vec<_>>();

        assert_eq!(actual_tokens, tokens);
    }
}

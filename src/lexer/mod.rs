#[cfg(test)]
mod tests;
pub mod token;

pub use token::*;

use miette::{LabeledSpan, Result, SourceSpan};

pub struct Lexer<'ast> {
    whole: &'ast str,
    remainder: &'ast str,
    byte_offset: usize,
    peeked: Option<Result<Token<'ast>, miette::Error>>,
}

impl<'ast> Lexer<'ast> {
    pub fn new(input: &'ast str) -> Self {
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
    ) -> Result<Token<'ast>, miette::Error> {
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
        mut check: impl FnMut(&Token<'ast>) -> bool,
        unexpected: &str,
    ) -> Result<Token<'ast>, miette::Error> {
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

    pub fn expect_any(&mut self, unexpected: &str) -> Result<Token<'ast>, miette::Error> {
        self.expect_where(|_| true, unexpected)
    }

    pub fn peek(&mut self) -> Option<&Result<Token<'ast>, miette::Error>> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        self.peeked = self.next();
        self.peeked.as_ref()
    }

    pub fn peek_expect(
        &mut self,
        expected: TokenKind,
    ) -> Option<&Result<Token<'ast>, miette::Error>> {
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
    ) -> Result<(TokenKind, TokenValue<'ast>)> {
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

impl<'ast> Iterator for Lexer<'ast> {
    type Item = Result<Token<'ast>>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.peeked.take() {
            return Some(next);
        }

        let mut chars = self.remainder.chars();

        let start_offset = self.byte_offset;

        let c = chars.next()?;
        self.remainder = chars.as_str();
        self.byte_offset += c.len_utf8();

        let kind: Result<(TokenKind, TokenValue<'ast>)> = match c {
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

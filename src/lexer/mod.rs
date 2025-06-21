use crate::prelude::*;
mod token;
pub use token::*;

pub struct Lexer<'input> {
    pub source_code: &'input str,
    pub remainder: &'input str,
    pub byte_offset: usize,
    pub peeked: Option<Result<Token>>,
}

impl<'input> Lexer<'input> {
    pub fn new(source_code: &'input str) -> Lexer<'input> {
        Lexer {
            source_code,
            remainder: source_code,
            byte_offset: 0,
            peeked: None,
        }
    }

    pub fn peek(&mut self) -> Option<&Result<Token>> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        self.peeked = self.next();
        self.peeked.as_ref()
    }

    fn parse_compound_operator(
        &mut self,
        single: TokenKind,
        compound: TokenKind,
        expected_char: char,
    ) -> Result<(TokenKind, TokenValue)> {
        if let Some(c) = self.remainder.chars().next() {
            if c == expected_char {
                self.remainder = self.remainder[c.len_utf8()..].into();
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

impl Iterator for Lexer<'_> {
    type Item = Result<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.peeked.take() {
            return Some(next);
        }

        let mut chars = self.remainder.chars();

        let start_offset = self.byte_offset;

        let c = chars.next()?;
        self.remainder = chars.as_str();
        self.byte_offset += c.len_utf8();

        let kind: Result<(TokenKind, TokenValue)> = match c {
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
            '`' => Ok((TokenKind::Tick, TokenValue::None)),
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
                        self.remainder = self.remainder[c.len_utf8()..].into();
                        self.byte_offset += c.len_utf8();
                    } else {
                        break;
                    }
                }

                match ident.as_str() {
                    "if" => Ok((TokenKind::If, TokenValue::None)),
                    "else" => Ok((TokenKind::Else, TokenValue::None)),
                    "intrinsic" => Ok((TokenKind::Intrinsic, TokenValue::None)),
                    "fn" => Ok((TokenKind::Function, TokenValue::None)),
                    "true" => Ok((TokenKind::Boolean, TokenValue::Boolean(true))),
                    "false" => Ok((TokenKind::Boolean, TokenValue::Boolean(false))),
                    "let" => Ok((TokenKind::Let, TokenValue::None)),
                    "while" => Ok((TokenKind::While, TokenValue::None)),
                    "for" => Ok((TokenKind::For, TokenValue::None)),
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
                    "unit" => Ok((TokenKind::UnitType, TokenValue::None)),
                    "use" => Ok((TokenKind::Use, TokenValue::None)),
                    "mod" => Ok((TokenKind::Mod, TokenValue::None)),
                    _ => Ok((
                        TokenKind::Identifier,
                        TokenValue::Identifier(Identifier {
                            name: ident.clone().into(),
                            span: Span::new(start_offset.into(), ident.len()),
                        }),
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
                        self.remainder = self.remainder[c.len_utf8()..].into();
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
                    Err(lexer_improper_number(
                        &number,
                        (self.byte_offset - number.len(), self.byte_offset),
                    )
                    .into())
                }
            }
            '"' => {
                let mut string = String::new();
                while let Some(c) = self.remainder.chars().next() {
                    if c == '"' {
                        self.remainder = self.remainder[c.len_utf8()..].into();
                        self.byte_offset += c.len_utf8();
                        break;
                    } else {
                        string.push(c);
                        self.remainder = self.remainder[c.len_utf8()..].into();
                        self.byte_offset += c.len_utf8();
                    }
                }

                Ok((TokenKind::String, TokenValue::String(string.into())))
            }
            '\'' => {
                let c = self.remainder.chars().next()?;
                self.remainder = self.remainder[c.len_utf8()..].into();
                self.byte_offset += c.len_utf8();

                if self.remainder.chars().next()? == '\'' {
                    // advance past the closing quote, not the character itself
                    self.remainder = self.remainder['\''.len_utf8()..].into();
                    self.byte_offset += '\''.len_utf8();
                    Ok((TokenKind::Character, TokenValue::Character(c)))
                } else {
                    Err(lexer_improper_character(
                        &c.to_string(),
                        (self.byte_offset - c.len_utf8(), c.len_utf8()),
                    )
                    .into())
                }
            }
            ' ' | '\r' | '\t' | '\n' => {
                return self.next();
            }
            c => Err(lexer_unexpected_character(
                c,
                (self.byte_offset - c.len_utf8(), c.len_utf8()),
            )
            .into()),
        };

        let byte_length = self
            .byte_offset
            .checked_sub(start_offset)
            .expect("byte_offset should never be less than start_offset");

        Some(kind.map(|(kind, value)| Token {
            kind,
            value,
            span: Span::new(start_offset.into(), byte_length),
            original: self.source_code[start_offset..self.byte_offset].into(),
        }))
    }
}

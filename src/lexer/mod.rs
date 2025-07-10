use crate::prelude::*;
mod token;
pub use token::*;

pub struct Lexer<'input> {
    pub source_code: &'input str,
    pub remainder: &'input str,
    pub byte_offset: usize,
    pub peeked: Option<Result<Token>>,
    current: Option<Result<Token>>,
}

impl<'input> Lexer<'input> {
    pub fn new(source: &'input str) -> Lexer<'input> {
        Lexer {
            source_code: source,
            remainder: source,
            byte_offset: 0,
            peeked: None,
            current: None,
        }
    }

    pub fn peek(&mut self) -> Option<&Result<Token>> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        self.peeked = self.next();
        self.peeked.as_ref()
    }

    pub fn current(&mut self) -> Option<Result<Token>> {
        self.current.clone()
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
            self.current = Some(next.clone());
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
                    "extern" => Ok((TokenKind::Extern, TokenValue::None)),
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
                    "i32" => Ok((TokenKind::I32Type, TokenValue::None)),
                    "i64" => Ok((TokenKind::I64Type, TokenValue::None)),
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
                // scan number (int or decimal) into `number_str`, update remainder & byte_offset...
                let mut number_str = String::new();
                number_str.push(c);
                while let Some(next_c) = self.remainder.chars().next() {
                    if next_c.is_ascii_digit() || next_c == '.' {
                        number_str.push(next_c);
                        self.remainder = &self.remainder[next_c.len_utf8()..];
                        self.byte_offset += next_c.len_utf8();
                    } else {
                        break;
                    }
                }
                // after digits/fraction, try to read a suffix
                let mut suffix = String::new();
                while let Some(c) = self.remainder.chars().next() {
                    // allow letters and digits in suffix (e.g. "i64")
                    if c.is_ascii_alphabetic() || c.is_ascii_digit() {
                        suffix.push(c);
                        self.remainder = &self.remainder[c.len_utf8()..];
                        self.byte_offset += c.len_utf8();
                    } else {
                        break;
                    }
                }
                // now decide based on suffix
                let (kind, value) = match suffix.as_str() {
                    "" => {
                        // no suffix: default integer or decimal
                        if number_str.contains('.') {
                            (
                                TokenKind::Decimal,
                                TokenValue::Decimal(number_str.parse().unwrap()),
                            )
                        } else {
                            (TokenKind::I32, TokenValue::I32(number_str.parse().unwrap()))
                        }
                    }
                    "i32" => (TokenKind::I32, TokenValue::I32(number_str.parse().unwrap())),
                    "i64" => (TokenKind::I64, TokenValue::I64(number_str.parse().unwrap())),
                    other => {
                        return Some(Err(lexer_improper_number(
                            &number_str,
                            (self.byte_offset - number_str.len(), number_str.len()),
                        )))
                    }
                };
                Ok((kind, value))
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

        let token = Some(kind.map(|(kind, value)| Token {
            kind,
            value,
            span: Span::new(start_offset.into(), byte_length),
            original: self.source_code[start_offset..self.byte_offset].into(),
        }));

        self.current = token.clone();
        token
    }
}

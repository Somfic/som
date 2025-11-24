mod identifier;
mod path;
#[cfg(test)]
mod tests;
mod token;

pub use identifier::*;
pub use path::*;

use std::sync::Arc;

pub use token::Token;
pub use token::TokenKind;
pub use token::TokenValue;

use crate::span::Position;
use crate::Result;
use crate::Source;
use crate::Span;

#[derive(Clone)]
pub struct Cursor {
    pub byte_offset: usize,
    pub position: Position,
    pub source: Arc<Source>,
}

pub struct Lexer {
    pub source: Arc<Source>,
    pub remainder: String,
    pub cursor: Cursor,
    pub peeked: Option<Token>,
    pub current: Option<Result<Token>>,
    pub source_content: Arc<str>,
}

impl Lexer {
    pub fn new(source: impl Into<Arc<Source>>) -> Lexer {
        let source = source.into();

        Lexer {
            remainder: source.content().to_string(),
            source: source.clone(),
            cursor: Cursor {
                byte_offset: 0,
                position: Position { line: 1, col: 1 },
                source: source.clone(),
            },
            peeked: None,
            current: None,
            source_content: source.content(),
        }
    }

    pub fn remainder(&self) -> &str {
        &self.source_content[self.cursor.byte_offset..]
    }

    pub fn peek(&mut self) -> Option<&Token> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        // Peek ahead without affecting current
        let prev_current = self.current.take();
        self.peeked = self.next().and_then(|r| r.ok());
        self.current = prev_current;
        self.peeked.as_ref()
    }

    pub fn current(&mut self) -> Option<&Token> {
        self.current.as_ref().and_then(|r| r.as_ref().ok())
    }

    fn consume_char(&mut self, c: char) {
        self.remainder = self.remainder[c.len_utf8()..].to_owned();
        self.cursor.byte_offset += c.len_utf8();

        if c == '\n' {
            self.cursor.position.line += 1;
            self.cursor.position.col = 1;
        } else {
            self.cursor.position.col += 1;
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.remainder.chars().next()
    }

    /// Create a span from start position to current cursor position
    fn make_span(&self, start_line: usize, start_col: usize, start_offset: usize) -> Span {
        let end_line = self.cursor.position.line;
        let end_col = self.cursor.position.col - 1;
        let length = self.cursor.byte_offset - start_offset;

        Span::new(
            start_line,
            start_col,
            end_line,
            end_col,
            start_offset,
            length,
            self.source.clone(),
        )
    }

    fn parse_compound_operator(
        &mut self,
        single: TokenKind,
        compound: TokenKind,
        expected_char: char,
    ) -> (TokenKind, TokenValue) {
        if let Some(c) = self.peek_char() {
            if c == expected_char {
                self.consume_char(c);
                (compound, TokenValue::None)
            } else {
                (single, TokenValue::None)
            }
        } else {
            (single, TokenValue::None)
        }
    }

    /// Skip whitespace characters
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if matches!(c, ' ' | '\t' | '\r' | '\n') {
                self.consume_char(c);
            } else {
                break;
            }
        }
    }

    /// Parse an escape sequence, returns the resulting character or an error message
    fn parse_escape_sequence(&mut self) -> std::result::Result<char, String> {
        match self.peek_char() {
            Some('n') => {
                self.consume_char('n');
                Ok('\n')
            }
            Some('r') => {
                self.consume_char('r');
                Ok('\r')
            }
            Some('t') => {
                self.consume_char('t');
                Ok('\t')
            }
            Some('\\') => {
                self.consume_char('\\');
                Ok('\\')
            }
            Some('\'') => {
                self.consume_char('\'');
                Ok('\'')
            }
            Some('"') => {
                self.consume_char('"');
                Ok('"')
            }
            Some('0') => {
                self.consume_char('0');
                Ok('\0')
            }
            Some('x') => {
                self.consume_char('x');
                // Parse exactly 2 hex digits
                let mut hex = String::new();
                for _ in 0..2 {
                    if let Some(c) = self.peek_char() {
                        if c.is_ascii_hexdigit() {
                            hex.push(c);
                            self.consume_char(c);
                        } else {
                            return Err(format!("invalid hex escape sequence: \\x{}", hex));
                        }
                    } else {
                        return Err(format!("incomplete hex escape sequence: \\x{}", hex));
                    }
                }
                u8::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(|n| char::from_u32(n as u32))
                    .ok_or_else(|| format!("invalid hex escape sequence: \\x{}", hex))
            }
            Some('u') => {
                self.consume_char('u');
                if self.peek_char() != Some('{') {
                    return Err("unicode escape must start with \\u{".to_string());
                }
                self.consume_char('{');

                let mut hex = String::new();
                while let Some(c) = self.peek_char() {
                    if c == '}' {
                        self.consume_char('}');
                        break;
                    }
                    if c.is_ascii_hexdigit() && hex.len() < 6 {
                        hex.push(c);
                        self.consume_char(c);
                    } else {
                        return Err(format!("invalid unicode escape sequence: \\u{{{}}}", hex));
                    }
                }

                if hex.is_empty() {
                    return Err("empty unicode escape sequence".to_string());
                }

                u32::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .ok_or_else(|| format!("invalid unicode escape sequence: \\u{{{}}}", hex))
            }
            Some(c) => Err(format!("unknown escape sequence: \\{}", c)),
            None => Err("unterminated escape sequence".to_string()),
        }
    }

    /// Parse a string literal with escape sequences
    fn parse_string(&mut self) -> (TokenKind, TokenValue) {
        let mut string = String::new();

        loop {
            match self.peek_char() {
                Some('"') => {
                    self.consume_char('"');
                    return (TokenKind::String, TokenValue::String(string.into()));
                }
                Some('\\') => {
                    self.consume_char('\\');
                    match self.parse_escape_sequence() {
                        Ok(c) => string.push(c),
                        Err(msg) => {
                            // On error, consume the rest of the string and return error token
                            while let Some(c) = self.peek_char() {
                                if c == '"' {
                                    self.consume_char('"');
                                    break;
                                }
                                self.consume_char(c);
                            }
                            return (TokenKind::Error, TokenValue::Error(msg.into_boxed_str()));
                        }
                    }
                }
                Some(c) => {
                    string.push(c);
                    self.consume_char(c);
                }
                None => {
                    return (
                        TokenKind::Error,
                        TokenValue::Error("unterminated string literal".into()),
                    );
                }
            }
        }
    }

    /// Parse a character literal with escape sequences
    fn parse_char(&mut self) -> (TokenKind, TokenValue) {
        let c = match self.peek_char() {
            Some('\\') => {
                self.consume_char('\\');
                match self.parse_escape_sequence() {
                    Ok(c) => c,
                    Err(msg) => {
                        // Consume until closing quote if present
                        while let Some(ch) = self.peek_char() {
                            self.consume_char(ch);
                            if ch == '\'' {
                                break;
                            }
                        }
                        return (TokenKind::Error, TokenValue::Error(msg.into_boxed_str()));
                    }
                }
            }
            Some(c) => {
                self.consume_char(c);
                c
            }
            None => {
                return (
                    TokenKind::Error,
                    TokenValue::Error("unterminated character literal".into()),
                );
            }
        };

        // Expect closing quote
        if self.peek_char() == Some('\'') {
            self.consume_char('\'');
            (TokenKind::Character, TokenValue::Character(c))
        } else {
            // Consume the rest until we find a quote or run out
            while let Some(ch) = self.peek_char() {
                self.consume_char(ch);
                if ch == '\'' {
                    break;
                }
            }
            (
                TokenKind::Error,
                TokenValue::Error("character literal must contain exactly one character".into()),
            )
        }
    }

    /// Parse an identifier or keyword
    fn parse_identifier(&mut self, first_char: char) -> (TokenKind, TokenValue, usize, usize) {
        let start_offset = self.cursor.byte_offset - first_char.len_utf8();
        let start_line = self.cursor.position.line;
        let start_col = self.cursor.position.col - 1; // We already consumed first_char

        let mut ident = String::new();
        ident.push(first_char);

        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                ident.push(c);
                self.consume_char(c);
            } else {
                break;
            }
        }

        let (kind, value) = match ident.as_str() {
            "if" => (TokenKind::If, TokenValue::None),
            "else" => (TokenKind::Else, TokenValue::None),
            "extern" => (TokenKind::Extern, TokenValue::None),
            "as" => (TokenKind::As, TokenValue::None),
            "pub" => (TokenKind::Pub, TokenValue::None),
            "fn" => (TokenKind::Function, TokenValue::None),
            "true" => (TokenKind::Boolean, TokenValue::Boolean(true)),
            "false" => (TokenKind::Boolean, TokenValue::Boolean(false)),
            "let" => (TokenKind::Let, TokenValue::None),
            "while" => (TokenKind::While, TokenValue::None),
            "for" => (TokenKind::For, TokenValue::None),
            "type" => (TokenKind::Type, TokenValue::None),
            "struct" => (TokenKind::Struct, TokenValue::None),
            "enum" => (TokenKind::Enum, TokenValue::None),
            "trait" => (TokenKind::Trait, TokenValue::None),
            "bool" => (TokenKind::BooleanType, TokenValue::None),
            "byte" => (TokenKind::ByteType, TokenValue::None),
            "i32" => (TokenKind::I32Type, TokenValue::None),
            "i64" => (TokenKind::I64Type, TokenValue::None),
            "f64" => (TokenKind::F64Type, TokenValue::None),
            "str" => (TokenKind::StringType, TokenValue::None),
            "char" => (TokenKind::CharacterType, TokenValue::None),
            "return" => (TokenKind::Return, TokenValue::None),
            "unit" => (TokenKind::UnitType, TokenValue::None),
            "use" => (TokenKind::Use, TokenValue::None),
            "mod" => (TokenKind::Mod, TokenValue::None),
            _ => {
                let span = self.make_span(start_line, start_col, start_offset);
                (
                    TokenKind::Identifier,
                    TokenValue::Identifier(Identifier {
                        name: ident.clone().into(),
                        span,
                    }),
                )
            }
        };

        (kind, value, start_offset, ident.len())
    }

    /// Parse a number literal (integer or decimal)
    fn parse_number(&mut self, first_char: char) -> (TokenKind, TokenValue) {
        let mut number_str = String::new();
        number_str.push(first_char);

        // Scan digits and optional decimal point
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' {
                number_str.push(c);
                self.consume_char(c);
            } else {
                break;
            }
        }

        // Try to read a suffix
        let mut suffix = String::new();
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() {
                suffix.push(c);
                self.consume_char(c);
            } else {
                break;
            }
        }

        // Determine the token kind and value based on suffix
        match suffix.as_str() {
            "" => {
                if number_str.contains('.') {
                    match number_str.parse::<f64>() {
                        Ok(value) => (TokenKind::F64, TokenValue::Decimal(value)),
                        Err(_) => (
                            TokenKind::Error,
                            TokenValue::Error(
                                format!("invalid decimal number: {}", number_str).into(),
                            ),
                        ),
                    }
                } else {
                    match number_str.parse::<i32>() {
                        Ok(value) => (TokenKind::I32, TokenValue::I32(value)),
                        Err(_) => match number_str.parse::<i64>() {
                            Ok(value) => (TokenKind::I64, TokenValue::I64(value)),
                            Err(_) => (
                                TokenKind::Error,
                                TokenValue::Error(format!("invalid number: {}", number_str).into()),
                            ),
                        },
                    }
                }
            }
            "i" => match number_str.parse::<i32>() {
                Ok(value) => (TokenKind::I32, TokenValue::I32(value)),
                Err(_) => (
                    TokenKind::Error,
                    TokenValue::Error(format!("invalid i32 number: {}", number_str).into()),
                ),
            },
            "l" => match number_str.parse::<i64>() {
                Ok(value) => (TokenKind::I64, TokenValue::I64(value)),
                Err(_) => (
                    TokenKind::Error,
                    TokenValue::Error(format!("invalid i64 number: {}", number_str).into()),
                ),
            },
            _ => (
                TokenKind::Error,
                TokenValue::Error(format!("invalid number suffix: {}", suffix).into()),
            ),
        }
    }
}

impl Iterator for Lexer {
    type Item = Result<Token>;

    fn next(&mut self) -> Option<Self::Item> {
        // Return peeked token if available
        if let Some(next) = self.peeked.take() {
            self.current = Some(Ok(next.clone()));
            return Some(Ok(next));
        }

        // Skip whitespace and comments
        loop {
            self.skip_whitespace();

            // Check for comments
            if self.peek_char() == Some('/') {
                let next_char = self.remainder.chars().nth(1);

                if next_char == Some('/') {
                    // Single-line comment
                    self.consume_char('/');
                    self.consume_char('/');

                    while let Some(c) = self.peek_char() {
                        if c == '\n' {
                            break;
                        }
                        self.consume_char(c);
                    }
                    continue; // Skip this comment and try again
                } else if next_char == Some('*') {
                    // Multi-line comment
                    let start_line = self.cursor.position.line;
                    let start_col = self.cursor.position.col;
                    let start_offset = self.cursor.byte_offset;

                    self.consume_char('/');
                    self.consume_char('*');

                    let mut found_end = false;
                    while let Some(c) = self.peek_char() {
                        if c == '*' && self.remainder.chars().nth(1) == Some('/') {
                            self.consume_char('*');
                            self.consume_char('/');
                            found_end = true;
                            break;
                        }
                        self.consume_char(c);
                    }

                    if !found_end {
                        // Return error token for unterminated comment
                        let span = self.make_span(start_line, start_col, start_offset);
                        let original =
                            &self.source.content()[start_offset..self.cursor.byte_offset];
                        let token = Token {
                            kind: TokenKind::Error,
                            value: TokenValue::Error("unterminated block comment".into()),
                            span,
                            original: original.into(),
                        };
                        self.current = Some(Ok(token.clone()));
                        return Some(Ok(token));
                    }
                    continue; // Skip this comment and try again
                }
            }

            // No more whitespace or comments, break out
            break;
        }

        // Record start position
        let start_offset = self.cursor.byte_offset;
        let start_line = self.cursor.position.line;
        let start_col = self.cursor.position.col;

        // Get next character
        let c = self.peek_char()?;
        self.consume_char(c);

        let (kind, value) = match c {
            // Delimiters
            '(' => (TokenKind::ParenOpen, TokenValue::None),
            ')' => (TokenKind::ParenClose, TokenValue::None),
            '{' => (TokenKind::CurlyOpen, TokenValue::None),
            '}' => (TokenKind::CurlyClose, TokenValue::None),
            '[' => (TokenKind::SquareOpen, TokenValue::None),
            ']' => (TokenKind::SquareClose, TokenValue::None),
            ';' => (TokenKind::Semicolon, TokenValue::None),
            ',' => (TokenKind::Comma, TokenValue::None),
            '.' => (TokenKind::Dot, TokenValue::None),
            ':' => self.parse_compound_operator(TokenKind::Colon, TokenKind::DoubleColon, ':'),

            // Special symbols
            '@' => (TokenKind::At, TokenValue::None),
            '#' => (TokenKind::Hash, TokenValue::None),
            '$' => (TokenKind::Dollar, TokenValue::None),
            '^' => (TokenKind::Caret, TokenValue::None),
            '`' => (TokenKind::Tick, TokenValue::None),
            '~' => (TokenKind::Tilde, TokenValue::None),
            '?' => (TokenKind::Question, TokenValue::None),

            // Operators (simple)
            '+' => (TokenKind::Plus, TokenValue::None),
            '*' => (TokenKind::Star, TokenValue::None),
            '/' => (TokenKind::Slash, TokenValue::None),
            '%' => (TokenKind::Percent, TokenValue::None),

            // Compound operators
            '-' => self.parse_compound_operator(TokenKind::Minus, TokenKind::Arrow, '>'),
            '&' => self.parse_compound_operator(TokenKind::Ampersand, TokenKind::And, '&'),
            '|' => self.parse_compound_operator(TokenKind::Pipe, TokenKind::Or, '|'),
            '=' => self.parse_compound_operator(TokenKind::Equal, TokenKind::Equality, '='),
            '!' => self.parse_compound_operator(TokenKind::Bang, TokenKind::Inequality, '='),
            '<' => {
                self.parse_compound_operator(TokenKind::LessThan, TokenKind::LessThanOrEqual, '=')
            }
            '>' => self.parse_compound_operator(
                TokenKind::GreaterThan,
                TokenKind::GreaterThanOrEqual,
                '=',
            ),

            // String literals
            '"' => self.parse_string(),

            // Character literals
            '\'' => self.parse_char(),

            // Identifiers and keywords
            'a'..='z' | 'A'..='Z' | '_' => {
                let (kind, value, id_start, id_len) = self.parse_identifier(c);
                // For identifiers, we need to return early to use the correct span
                let span = self.make_span(start_line, start_col, start_offset);
                let original = &self.source.content()[id_start..id_start + id_len];
                let token = Token {
                    kind,
                    value,
                    span,
                    original: original.into(),
                };
                self.current = Some(Ok(token.clone()));
                return Some(Ok(token));
            }

            // Number literals
            '0'..='9' => self.parse_number(c),

            // Unknown character
            c => (
                TokenKind::Error,
                TokenValue::Error(format!("unexpected character: '{}'", c).into()),
            ),
        };

        // Create token with span
        let span = self.make_span(start_line, start_col, start_offset);
        let original = &self.source.content()[start_offset..self.cursor.byte_offset];

        let token = Token {
            kind,
            value,
            span,
            original: original.into(),
        };

        self.current = Some(Ok(token.clone()));
        Some(Ok(token))
    }
}

use lexeme::Lexeme;
use lexeme::Range;
use lexeme::Token;
use lexeme::TokenType;
use lexeme::TokenValue;
use regex::Regex;

pub mod lexeme;

type SpecItem = (Regex, fn(&str) -> (TokenType, TokenValue));

macro_rules! r {
    ($pattern:expr) => {
        Regex::new(format!("^{}", $pattern).as_str()).unwrap()
    };
}

pub struct Scanner {
    input: String,
    cursor: usize,
    spec: Vec<SpecItem>,
}

impl Scanner {
    pub fn new(input: String) -> Scanner {
        Scanner {
            input,
            cursor: 0,
            spec: vec![
                (r!(r"(\s+)"), |_| (TokenType::Ignore, TokenValue::None)),
                (r!(r"//(.*)"), |_| (TokenType::Ignore, TokenValue::None)),
                (r!(r"(\()"), |_| (TokenType::ParenOpen, TokenValue::None)),
                (r!(r"(\))"), |_| (TokenType::ParenClose, TokenValue::None)),
                (r!(r"(\{)"), |_| (TokenType::CurlyOpen, TokenValue::None)),
                (r!(r"(\})"), |_| (TokenType::CurlyClose, TokenValue::None)),
                (r!(r"(\[)"), |_| (TokenType::SquareOpen, TokenValue::None)),
                (r!(r"(\])"), |_| (TokenType::SquareClose, TokenValue::None)),
                (r!(r"(\,)"), |_| (TokenType::Comma, TokenValue::None)),
                (r!(r"(\.)"), |_| (TokenType::Dot, TokenValue::None)),
                (r!(r"(\:)"), |_| (TokenType::Colon, TokenValue::None)),
                (r!(r"(;)"), |_| (TokenType::Semicolon, TokenValue::None)),
                (r!(r"(\+)"), |_| (TokenType::Plus, TokenValue::None)),
                (r!(r"(-)"), |_| (TokenType::Minus, TokenValue::None)),
                (r!(r"(/)"), |_| (TokenType::Slash, TokenValue::None)),
                (r!(r"(\*)"), |_| (TokenType::Star, TokenValue::None)),
                (r!(r"(=)"), |_| (TokenType::Equal, TokenValue::None)),
                (r!(r"(!)"), |_| (TokenType::Not, TokenValue::None)),
                (r!(r"(<)"), |_| (TokenType::LessThan, TokenValue::None)),
                (r!(r"(>)"), |_| (TokenType::GreaterThan, TokenValue::None)),
                (r!(r"(<=)"), |_| {
                    (TokenType::LessThanOrEqual, TokenValue::None)
                }),
                (r!(r"(>=)"), |_| {
                    (TokenType::GreaterThanOrEqual, TokenValue::None)
                }),
                (r!(r"(==)"), |_| (TokenType::Equality, TokenValue::None)),
                (r!(r"(!=)"), |_| (TokenType::Inequality, TokenValue::None)),
                (r!(r"(if)"), |_| (TokenType::If, TokenValue::None)),
                (r!(r"(else)"), |_| (TokenType::Else, TokenValue::None)),
                (r!(r"(while)"), |_| (TokenType::While, TokenValue::None)),
                (r!(r"(for)"), |_| (TokenType::For, TokenValue::None)),
                (r!(r"(let)"), |_| (TokenType::Let, TokenValue::None)),
                (r!(r"(fn)"), |_| (TokenType::Function, TokenValue::None)),
                (r!(r"(return)"), |_| (TokenType::Return, TokenValue::None)),
                (r!(r"(true)"), |_| {
                    (TokenType::Boolean, TokenValue::Boolean(true))
                }),
                (r!(r"(false)"), |_| {
                    (TokenType::Boolean, TokenValue::Boolean(false))
                }),
                (r!(r"(\d+\.\d+)"), |value| {
                    (
                        TokenType::Decimal,
                        TokenValue::Decimal(value.parse().unwrap()),
                    )
                }),
                (r!(r"(\d+)"), |value| {
                    (
                        TokenType::Integer,
                        TokenValue::Integer(value.parse().unwrap()),
                    )
                }),
                (r!(r"'([^']*)'"), |value| {
                    (TokenType::String, TokenValue::String(value.to_string()))
                }),
                (r!(r"`([^`]*)`"), |value| {
                    (
                        TokenType::Character,
                        TokenValue::Character(value.chars().next().unwrap()),
                    )
                }),
                (r!(r"([a-zA-Z_]\w*)"), |value| {
                    (
                        TokenType::Identifier,
                        TokenValue::Identifier(value.to_string()),
                    )
                }),
            ],
        }
    }

    fn find_lexeme(&self, cursor: usize) -> Option<(Lexeme, usize)> {
        let haystack = &self.input.chars().skip(cursor).collect::<String>();

        for (regex, handler) in &self.spec {
            let capture = regex.captures(haystack);

            if let Some((capture, matched)) = capture.and_then(|c| Some((c.get(0)?, c.get(1)?))) {
                let value = matched.as_str();
                let (token_type, token_value) = handler(value);
                let length = capture.as_str().chars().count(); // TODO: Check if we shouldn't use as_str().len() instead
                let new_cursor = cursor + capture.end();
                return Some((
                    Lexeme::valid(token_type, token_value, cursor, length),
                    new_cursor,
                ));
            }
        }

        None
    }
}

impl Iterator for Scanner {
    type Item = Lexeme;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.input.chars().count() {
            return None;
        }

        // Search for the next lexeme. If we get a None value, keep increasing the cursor until the next lexeme would be found. Return an Invalid Lexeme, and have the next call to this function handle the next valid lexeme.
        let lexeme = self.find_lexeme(self.cursor);
        if lexeme.is_none() {
            let cursor_start = self.cursor;
            let mut cursor = self.cursor;
            while self.find_lexeme(cursor).is_none() {
                cursor += 1;

                if cursor >= self.input.chars().count() {
                    break;
                }
            }

            let length = cursor - self.cursor;
            self.cursor = cursor;
            return Some(Lexeme::invalid(cursor_start, length));
        }

        let (lexeme, new_cursor) = lexeme.unwrap();
        self.cursor = new_cursor;

        if let Lexeme::Valid(token) = &lexeme {
            if token.token_type == TokenType::Ignore {
                return self.next();
            }
        }

        Some(lexeme)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_whitespace() {
        test_scanner("  \t\n", vec![]);
    }

    #[test]
    fn ignores_comments() {
        test_scanner("// this is a comment", vec![]);
    }

    #[test]
    fn parses_integers() {
        test_scanner(
            "123",
            vec![Lexeme::valid(
                TokenType::Integer,
                TokenValue::Integer(123),
                0,
                3,
            )],
        );
    }

    #[test]
    fn parses_decimals() {
        test_scanner(
            "123.456",
            vec![Lexeme::valid(
                TokenType::Decimal,
                TokenValue::Decimal(123.456),
                0,
                7,
            )],
        );
    }

    #[test]
    fn parses_strings() {
        test_scanner(
            "'hello'",
            vec![Lexeme::valid(
                TokenType::String,
                TokenValue::String("hello".to_string()),
                0,
                7,
            )],
        );
    }

    #[test]
    fn parses_characters() {
        test_scanner(
            "`a`",
            vec![Lexeme::valid(
                TokenType::Character,
                TokenValue::Character('a'),
                0,
                3,
            )],
        );
    }
    #[test]
    fn parses_emoji() {
        test_scanner(
            "`🦀`",
            vec![Lexeme::valid(
                TokenType::Character,
                TokenValue::Character('🦀'),
                0,
                3,
            )],
        );
    }

    #[test]
    fn parses_identifiers() {
        test_scanner(
            "foo",
            vec![Lexeme::valid(
                TokenType::Identifier,
                TokenValue::Identifier("foo".to_string()),
                0,
                3,
            )],
        );
    }

    #[test]
    fn parses_operators() {
        test_scanner(
            "+ - / * =",
            vec![
                Lexeme::valid(TokenType::Plus, TokenValue::None, 0, 1),
                Lexeme::valid(TokenType::Minus, TokenValue::None, 2, 1),
                Lexeme::valid(TokenType::Slash, TokenValue::None, 4, 1),
                Lexeme::valid(TokenType::Star, TokenValue::None, 6, 1),
                Lexeme::valid(TokenType::Equal, TokenValue::None, 8, 1),
            ],
        );
    }
    #[test]
    fn parses_parentheses() {
        test_scanner(
            "( )",
            vec![
                Lexeme::valid(TokenType::ParenOpen, TokenValue::None, 0, 1),
                Lexeme::valid(TokenType::ParenClose, TokenValue::None, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_curly_braces() {
        test_scanner(
            "{ }",
            vec![
                Lexeme::valid(TokenType::CurlyOpen, TokenValue::None, 0, 1),
                Lexeme::valid(TokenType::CurlyClose, TokenValue::None, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_multiple_tokens() {
        test_scanner(
            "123 + 456",
            vec![
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(123), 0, 3),
                Lexeme::valid(TokenType::Plus, TokenValue::None, 4, 1),
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(456), 6, 3),
            ],
        );
    }

    #[test]
    fn parses_invalid_lexeme() {
        test_scanner(
            "123~456",
            vec![
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(123), 0, 3),
                Lexeme::invalid(3, 1),
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(456), 4, 3),
            ],
        );
    }

    #[test]
    fn parses_invalid_lexeme_at_end() {
        test_scanner(
            "123~~~±±±",
            vec![
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(123), 0, 3),
                Lexeme::invalid(3, 6),
            ],
        );
    }

    #[test]
    fn parses_semicolons() {
        test_scanner(
            "123;456",
            vec![
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(123), 0, 3),
                Lexeme::valid(TokenType::Semicolon, TokenValue::None, 3, 1),
                Lexeme::valid(TokenType::Integer, TokenValue::Integer(456), 4, 3),
            ],
        );
    }

    fn test_scanner(input: &str, expected: Vec<Lexeme>) {
        let lexemes = Scanner::new(input.to_string()).collect::<Vec<_>>();

        assert_eq!(lexemes, expected);
    }
}

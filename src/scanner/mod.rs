use crate::files::Files;
use lexeme::Lexeme;
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

pub struct Scanner<'a> {
    files: &'a Files<'a>,
    spec: Vec<SpecItem>,
}

impl<'a> Scanner<'a> {
    pub fn new(files: &'a Files) -> Scanner<'a> {
        Scanner {
            files,
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
                (r!(r"(struct)"), |_| (TokenType::Struct, TokenValue::None)),
                (r!(r"(enum)"), |_| (TokenType::Enum, TokenValue::None)),
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

    pub fn parse(&self) -> Vec<Lexeme> {
        let mut lexemes = Vec::new();

        for file in self.files.file_ids() {
            let content = self.files.get(file).unwrap();
            let mut cursor = 0;

            let mut panic_start_at = None;
            while cursor < content.chars().count() {
                let haystack = content.chars().skip(cursor).collect::<String>();
                let mut was_matched = false;

                for (regex, handler) in &self.spec {
                    let capture = regex.captures(&haystack);

                    if let Some((capture, matched)) =
                        capture.and_then(|c| Some((c.get(0)?, c.get(1)?)))
                    {
                        if let Some(start) = panic_start_at.take() {
                            lexemes.push(Lexeme::invalid(file, start, cursor - start - 1));
                        }

                        let value = matched.as_str();
                        let (token_type, token_value) = handler(value);

                        if token_type == TokenType::Ignore {
                            was_matched = true;
                            cursor += capture.as_str().chars().count();
                            break;
                        }

                        let length = capture.as_str().chars().count();
                        let lexeme = Lexeme::valid(file, token_type, token_value, cursor, length);

                        lexemes.push(lexeme);

                        cursor += length;
                        was_matched = true;
                        break;
                    }
                }

                if !was_matched && panic_start_at.is_none() {
                    panic_start_at = Some(cursor);
                }

                if !was_matched {
                    cursor += 1;
                }
            }
        }

        lexemes
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
            vec![(TokenType::Integer, TokenValue::Integer(123), 0, 3)],
        );
    }

    #[test]
    fn parses_decimals() {
        test_scanner(
            "123.456",
            vec![(TokenType::Decimal, TokenValue::Decimal(123.456), 0, 7)],
        );
    }

    #[test]
    fn parses_strings() {
        test_scanner(
            "'hello'",
            vec![(
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
            vec![(TokenType::Character, TokenValue::Character('a'), 0, 3)],
        );
    }
    #[test]
    fn parses_emoji() {
        test_scanner(
            "`🦀`",
            vec![(TokenType::Character, TokenValue::Character('🦀'), 0, 3)],
        );
    }

    #[test]
    fn parses_identifiers() {
        test_scanner(
            "foo",
            vec![(
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
                (TokenType::Plus, TokenValue::None, 0, 1),
                (TokenType::Minus, TokenValue::None, 2, 1),
                (TokenType::Slash, TokenValue::None, 4, 1),
                (TokenType::Star, TokenValue::None, 6, 1),
                (TokenType::Equal, TokenValue::None, 8, 1),
            ],
        );
    }
    #[test]
    fn parses_parentheses() {
        test_scanner(
            "( )",
            vec![
                (TokenType::ParenOpen, TokenValue::None, 0, 1),
                (TokenType::ParenClose, TokenValue::None, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_curly_braces() {
        test_scanner(
            "{ }",
            vec![
                (TokenType::CurlyOpen, TokenValue::None, 0, 1),
                (TokenType::CurlyClose, TokenValue::None, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_multiple_tokens() {
        test_scanner(
            "123 + 456",
            vec![
                (TokenType::Integer, TokenValue::Integer(123), 0, 3),
                (TokenType::Plus, TokenValue::None, 4, 1),
                (TokenType::Integer, TokenValue::Integer(456), 6, 3),
            ],
        );
    }

    // #[test]
    // fn parses_invalid_lexeme() {
    //     test_scanner(
    //         "123~456",
    //         vec![
    //             (TokenType::Integer, TokenValue::Integer(123), 0, 3),
    //             Lexeme::invalid(3, 1),
    //             (TokenType::Integer, TokenValue::Integer(456), 4, 3),
    //         ],
    //     );
    // }

    // #[test]
    // fn parses_invalid_lexeme_at_end() {
    //     test_scanner(
    //         "123~~~±±±",
    //         vec![
    //             (TokenType::Integer, TokenValue::Integer(123), 0, 3),
    //             Lexeme::invalid(3, 6),
    //         ],
    //     );
    // }

    #[test]
    fn parses_semicolons() {
        test_scanner(
            "123;456",
            vec![
                (TokenType::Integer, TokenValue::Integer(123), 0, 3),
                (TokenType::Semicolon, TokenValue::None, 3, 1),
                (TokenType::Integer, TokenValue::Integer(456), 4, 3),
            ],
        );
    }

    #[test]
    fn parses_struct() {
        test_scanner("struct", vec![(TokenType::Struct, TokenValue::None, 0, 6)]);
    }

    fn test_scanner(input: &str, expected: Vec<(TokenType, TokenValue, usize, usize)>) {
        let mut files = Files::default();
        files.insert("test file", input);

        let scanner = Scanner::new(&files);
        let lexemes = scanner.parse();

        let expected = expected
            .into_iter()
            .map(|(token_type, token_value, cursor, length)| {
                Lexeme::valid("test file", token_type, token_value, cursor, length)
            })
            .collect::<Vec<_>>();

        assert_eq!(lexemes, expected);
    }
}

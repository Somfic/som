use regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub enum Lexeme {
    Valid(Token, Range),
    Invalid(Range),
}

impl Lexeme {
    pub fn valid(token: Token, start: usize, length: usize) -> Lexeme {
        Lexeme::Valid(
            token,
            Range {
                position: start,
                length,
            },
        )
    }

    pub fn invalid(start: usize, length: usize) -> Lexeme {
        Lexeme::Invalid(Range {
            position: start,
            length,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Range {
    pub position: usize,
    pub length: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Ignore,
    Number(i32),
    String(String),
    Character(char),
    Identifier(String),
    Equal,
    Plus,
    Minus,
    Slash,
    Star,
    ParenOpen,
    ParenClose,
    CurlyOpen,
    CurlyClose,
}

type SpecItem = (Regex, fn(&str) -> Token);

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
                (Regex::new(r#"^(\s+)"#).unwrap(), |_| Token::Ignore),
                (Regex::new(r#"^\/\/(.*)"#).unwrap(), |_| Token::Ignore),
                (Regex::new(r#"^(\d+)"#).unwrap(), |s: &str| {
                    Token::Number(s.parse().unwrap())
                }),
                (Regex::new(r#"^'([^"]*)'"#).unwrap(), |s: &str| {
                    Token::String(s.to_string())
                }),
                (Regex::new(r#"^`(.)`"#).unwrap(), |s: &str| {
                    Token::Character(s.chars().nth(0).unwrap())
                }),
                (Regex::new(r#"^([a-zA-Z_]\w*)"#).unwrap(), |s: &str| {
                    Token::Identifier(s.to_string())
                }),
                (Regex::new(r#"^(\+)"#).unwrap(), |_| Token::Plus),
                (Regex::new(r#"^(-)"#).unwrap(), |_| Token::Minus),
                (Regex::new(r#"^(\/)"#).unwrap(), |_| Token::Slash),
                (Regex::new(r#"^(\*)"#).unwrap(), |_| Token::Star),
                (Regex::new(r#"^(=)"#).unwrap(), |_| Token::Equal),
                (Regex::new(r#"^(?P<paren_open>\()"#).unwrap(), |_| {
                    Token::ParenOpen
                }),
                (Regex::new(r#"^(?P<paren_close>\))"#).unwrap(), |_| {
                    Token::ParenClose
                }),
                (Regex::new(r#"^(?P<curly_open>\{)"#).unwrap(), |_| {
                    Token::CurlyOpen
                }),
                (Regex::new(r#"^(?P<curly_close>\})"#).unwrap(), |_| {
                    Token::CurlyClose
                }),
            ],
        }
    }
}

impl Iterator for Scanner {
    type Item = Lexeme;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.input.chars().count() {
            return None;
        }

        let find_token = |input: &str, cursor: usize| -> Option<(Token, Range, usize)> {
            let haystack = &input.chars().skip(cursor).collect::<String>();

            for (regex, handler) in &self.spec {
                let capture = regex.captures(haystack);

                if let Some((capture, matched)) = capture.and_then(|c| Some((c.get(0)?, c.get(1)?)))
                {
                    let value = matched.as_str();
                    let token = handler(value);
                    let length = capture.as_str().chars().count(); // TODO: Check if we shouldn't use as_str().len() instead
                    let new_cursor = cursor + capture.end();
                    return Some((
                        token,
                        Range {
                            position: cursor,
                            length,
                        },
                        new_cursor,
                    ));
                }
            }

            None
        };

        // Search for the next lexeme. If we get a None value, keep increasing the cursor until the next lexeme would be found. Return an Invalid Lexeme, and have the next call to this function handle the next valid lexeme.
        let token = find_token(&self.input, self.cursor);
        if token.is_none() {
            let cursor_start = self.cursor;
            let mut cursor = self.cursor;
            while find_token(&self.input, cursor).is_none() {
                cursor += 1;

                if cursor >= self.input.chars().count() {
                    break;
                }
            }

            let length = cursor - self.cursor;
            self.cursor = cursor;
            return Some(Lexeme::invalid(cursor_start, length));
        }

        let (token, range, new_cursor) = token.unwrap();
        self.cursor = new_cursor;

        if token == Token::Ignore {
            self.next()
        } else {
            Some(Lexeme::valid(token, range.position, range.length))
        }
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
    fn parses_numbers() {
        test_scanner("123", vec![Lexeme::valid(Token::Number(123), 0, 3)]);
    }

    #[test]
    fn parses_strings() {
        test_scanner(
            "'hello'",
            vec![Lexeme::valid(Token::String("hello".to_string()), 0, 7)],
        );
    }

    #[test]
    fn parses_characters() {
        test_scanner("`a`", vec![Lexeme::valid(Token::Character('a'), 0, 3)]);
    }

    #[test]
    fn parses_emoji() {
        test_scanner("`🦀`", vec![Lexeme::valid(Token::Character('🦀'), 0, 3)]);
    }

    #[test]
    fn parses_identifiers() {
        test_scanner(
            "foo",
            vec![Lexeme::valid(Token::Identifier("foo".to_string()), 0, 3)],
        );
    }

    #[test]
    fn parses_operators() {
        test_scanner(
            "+ - / * =",
            vec![
                Lexeme::valid(Token::Plus, 0, 1),
                Lexeme::valid(Token::Minus, 2, 1),
                Lexeme::valid(Token::Slash, 4, 1),
                Lexeme::valid(Token::Star, 6, 1),
                Lexeme::valid(Token::Equal, 8, 1),
            ],
        );
    }

    #[test]
    fn parses_parentheses() {
        test_scanner(
            "( )",
            vec![
                Lexeme::valid(Token::ParenOpen, 0, 1),
                Lexeme::valid(Token::ParenClose, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_curly_braces() {
        test_scanner(
            "{ }",
            vec![
                Lexeme::valid(Token::CurlyOpen, 0, 1),
                Lexeme::valid(Token::CurlyClose, 2, 1),
            ],
        );
    }

    #[test]
    fn parses_multiple_tokens() {
        test_scanner(
            "123 + 456",
            vec![
                Lexeme::valid(Token::Number(123), 0, 3),
                Lexeme::valid(Token::Plus, 4, 1),
                Lexeme::valid(Token::Number(456), 6, 3),
            ],
        );
    }

    #[test]
    fn parsers_invalid_lexeme() {
        test_scanner(
            "123~456",
            vec![
                Lexeme::valid(Token::Number(123), 0, 3),
                Lexeme::invalid(3, 1),
                Lexeme::valid(Token::Number(456), 4, 3),
            ],
        );
    }

    #[test]
    fn parses_invalid_lexeme_at_end() {
        test_scanner(
            "123~~~",
            vec![
                Lexeme::valid(Token::Number(123), 0, 3),
                Lexeme::invalid(3, 3),
            ],
        );
    }

    fn test_scanner(input: &str, expected: Vec<Lexeme>) {
        let lexemes = Scanner::new(input.to_string()).collect::<Vec<_>>();

        assert_eq!(lexemes, expected,);
    }
}

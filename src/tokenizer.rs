use regex::Regex;

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

pub struct Tokenizer {
    input: String,
    cursor: usize,
    spec: Vec<SpecItem>,
}

impl Tokenizer {
    pub fn new(input: String) -> Tokenizer {
        Tokenizer {
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

impl Iterator for Tokenizer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.input.len() {
            return None;
        }

        let haystack = &self.input[self.cursor..];

        for (regex, handler) in &self.spec {
            let capture = regex.captures(haystack);

            if let Some((capture, matched)) = capture.and_then(|c| Some((c.get(0)?, c.get(1)?))) {
                let value = matched.as_str();
                let token = handler(value);
                self.cursor += capture.end();
                return Some(token);
            }
        }

        panic!("Unexpected token {}", haystack);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_whitespace() {
        test_tokenizer("  \t\n", vec![]);
    }

    #[test]
    fn ignores_comments() {
        test_tokenizer("// this is a comment", vec![]);
    }

    #[test]
    fn parses_numbers() {
        test_tokenizer("123", vec![Token::Number(123)]);
    }

    #[test]
    fn parses_strings() {
        test_tokenizer("'hello'", vec![Token::String("hello".to_string())]);
    }

    #[test]
    fn parses_characters() {
        test_tokenizer("`a`", vec![Token::Character('a')]);
    }

    #[test]
    fn parses_emoji() {
        test_tokenizer("`🦀`", vec![Token::Character('🦀')]);
    }

    #[test]
    fn parses_identifiers() {
        test_tokenizer("foo", vec![Token::Identifier("foo".to_string())]);
    }

    #[test]
    fn parses_operators() {
        test_tokenizer(
            "+ - / * =",
            vec![
                Token::Plus,
                Token::Minus,
                Token::Slash,
                Token::Star,
                Token::Equal,
            ],
        );
    }

    #[test]
    fn parses_parentheses() {
        test_tokenizer("( )", vec![Token::ParenOpen, Token::ParenClose]);
    }

    #[test]
    fn parses_curly_braces() {
        test_tokenizer("{ }", vec![Token::CurlyOpen, Token::CurlyClose]);
    }

    #[test]
    fn parses_multiple_tokens() {
        test_tokenizer(
            "123 + 456",
            vec![Token::Number(123), Token::Plus, Token::Number(456)],
        );
    }

    fn test_tokenizer(input: &str, expected: Vec<Token>) {
        let tokens: Vec<Token> = Tokenizer::new(input.to_string())
            .filter(|t| *t != Token::Ignore)
            .collect();

        assert_eq!(tokens, expected,);
    }
}

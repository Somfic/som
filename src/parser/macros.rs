#[allow(unused_macros)]
macro_rules! expect_statement {
    ($parser:expr, $cursor:expr) => {{
        crate::parser::statement::parse($parser, $cursor)
    }};
}

macro_rules! expect_expression {
    ($parser:expr, $cursor:expr, $binding_power:expr) => {{
        crate::parser::expression::parse($parser, $cursor, &$binding_power)
    }};
}

macro_rules! expect_type {
    ($parser:expr, $cursor:expr, $binding_power:expr) => {{
        crate::parser::typing::parse($parser, $cursor, &$binding_power)
    }};
}

macro_rules! expect_valid_token {
    ($parser:expr, $cursor:expr) => {{
        let lexeme = $parser.lexemes.get($cursor);

        match lexeme {
            Some(lexeme) => match lexeme {
                Lexeme::Valid(token) => Ok((token, lexeme.range())),
                Lexeme::Invalid(_) => Err(crate::diagnostic::Error::primary(
                    lexeme.range().file_id,
                    $cursor,
                    1,
                    "Invalid token",
                )),
            },
            None => Err(crate::diagnostic::Error::primary(
                $parser.lexemes.get(0).unwrap().range().file_id,
                $cursor + 1,
                0,
                "Unexpected end of file",
            )),
        }
    }};
}

// allows for multiple token types to be expected
// peek_any_token!(parser, cursor, TokenType::Plus, TokenType::Minus);
macro_rules! expect_any_token {
    ($parser:expr, $cursor:expr, $($token_type:expr),*) => {{
        let expected_token_types = vec![$($token_type.to_string()),*];

        let lexeme = $parser.lexemes.get($cursor);

        match lexeme {
            Some(lexeme) => match lexeme {
                Lexeme::Valid(token) => {
                    if expected_token_types.contains(&token.token_type.to_string()) {
                        Ok((token, $cursor + 1))
                    } else {
                        Err(crate::diagnostic::Error::primary(
                            lexeme.range().file_id,
                            $cursor,
                            1,
                            format!("Expected {}", expected_token_types.join(" or ")),
                        ))
                    }
                }
                Lexeme::Invalid(_) => Err(crate::diagnostic::Error::primary(
                    lexeme.range().file_id,
                    $cursor,
                    1,
                    "Invalid token",
                )),
            },
            None => Err(crate::diagnostic::Error::primary(
                $parser.lexemes.get(0).unwrap().range().file_id,
                $cursor + 1,
                0,
                "Unexpected end of file",
            )),
        }
    }};
}

#[allow(unused_macros)]
macro_rules! expect_optional_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let result = expect_tokens!($parser, $cursor, $token_type);

        match result {
            Ok((token, cursor)) => Ok((Some(token), cursor)),
            Err(_) => Ok((None, $cursor)),
        }
    }};
}
macro_rules! expect_token_value {
    ($token:expr, $value:path) => {{
        match &$token.value {
            $value(value) => value.clone(),
            _ => panic!("expect_token_value! should only return identifiers"),
        }
    }};
}

macro_rules! expect_tokens {
    ($parser:expr, $cursor:expr, $($token_type:expr),*) => {{
        let mut i = $cursor;
        let mut tokens = Vec::new();
        let mut valid = 0;

        $(
            let lexeme: Option<&crate::scanner::lexeme::Lexeme> = $parser.lexemes.get(i);

            match lexeme {
                Some(crate::scanner::lexeme::Lexeme::Valid(token)) => {
                    if token.token_type == $token_type {
                        tokens.push(token.clone());
                        valid += 1;
                    }
                }
                _ => {}
            };

            i += 1;
        )*

        let all_tokens = vec![$($token_type),*];

        if valid == all_tokens.len() {
            Ok((tokens, i))
        } else {
            let unexpected_tokens = all_tokens.iter().skip(valid).map(|t| t.to_string()).collect::<Vec<_>>();
            Err(crate::diagnostic::Error::primary($parser.lexemes.get(0).unwrap().range().file_id, $cursor + valid, 1, format!("Expected {}", unexpected_tokens.join(" and "))))
        }
    }};
}

pub(crate) use expect_any_token;
pub(crate) use expect_expression;
#[allow(unused_imports)]
pub(crate) use expect_optional_token;
#[allow(unused_imports)]
pub(crate) use expect_statement;
pub(crate) use expect_token_value;
pub(crate) use expect_tokens;
pub(crate) use expect_type;
pub(crate) use expect_valid_token;

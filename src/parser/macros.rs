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
        let token = $parser.tokens.get($cursor);

        match token {
            Some(token) => Ok((token, &token.range)),
            None => Err(vec![crate::diagnostic::Error::primary(
                $parser.tokens.get(0).unwrap().range.file_id,
                $cursor + 1,
                0,
                "Unexpected end of file",
            )]),
        }
    }};
}

// allows for multiple token types to be expected
// peek_any_token!(parser, cursor, TokenType::Plus, TokenType::Minus);
macro_rules! expect_any_token {
    ($parser:expr, $cursor:expr, $($token_type:expr),*) => {{
        let expected_token_types = vec![$($token_type.to_string()),*];

        let token = $parser.tokens.get($cursor);

        match token {
            Some(token) => {
                    if expected_token_types.contains(&token.token_type.to_string()) {
                        Ok((token, $cursor + 1))
                    } else {
                        Err(vec![crate::diagnostic::Error::primary(
                            token.range.file_id,
                            $cursor,
                            1,
                            format!("Expected {}", expected_token_types.join(" or ")),
                        )])
                    }
                }
            None => Err(vec![crate::diagnostic::Error::primary(
                $parser.tokens.get(0).unwrap().range.file_id,
                $cursor + 1,
                0,
                "Unexpected end of file",
            )]),
        }
    }};
}

#[allow(unused_macros)]
macro_rules! expect_optional_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let result = expect_tokens!($parser, $cursor, $token_type);

        match result {
            Ok((token, cursor)) => (Some(token[0].clone()), cursor),
            Err(_) => (None, $cursor),
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

        let mut invalid_indecies = Vec::new();

        $(
            let token: Option<&crate::scanner::lexeme::Token> = $parser.tokens.get(i);

            match token {
                Some(token) => {
                    if token.token_type == $token_type {
                        tokens.push(token.clone());
                    } else {
                        invalid_indecies.push((i, $token_type));
                    }
                }
                _ => {}
            };

            i += 1;
        )*

        if invalid_indecies.is_empty() {
            Ok((tokens, i))
        } else {

            let mut errors = Vec::new();

            for (invalid_index, expected_token_type) in invalid_indecies {
                let actual_token = $parser.tokens.get(invalid_index).unwrap();

                errors.push(crate::diagnostic::Error::primary(
                    $parser.tokens.get(0).unwrap().range.file_id,
                    $cursor + invalid_index,
                    1,
                    format!("Expected {}", expected_token_type)
                ).with_note(
                    format!("Expected {}, got {}", expected_token_type, actual_token.token_type)
                ));
            }

            Err(errors)
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

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

        if lexeme.is_none() {
            return Err(Diagnostic::error($cursor, 0, "Unexpected end of file"));
        }

        let lexeme = lexeme.unwrap();

        match lexeme {
            Lexeme::Valid(token) => (token, lexeme.range()),
            Lexeme::Invalid(_) => return Err(Diagnostic::error($cursor, 1, "Invalid token")),
        }
    }};
}

// allows for multiple token types to be expected
// peek_any_token!(parser, cursor, TokenType::Plus, TokenType::Minus);
macro_rules! expect_any_token {
    ($parser:expr, $cursor:expr, $($token_type:expr),*) => {{
        let expected_token_types = vec![$($token_type.to_string()),*];

        let expected_tokens = vec![$(expect_token!($parser, $cursor, $token_type)),*];

        // If any of the expected tokens are valid, return the first valid token
        match expected_tokens.into_iter().find(|token| token.is_ok()) {
            Some(token) => token,
            None => Err(Diagnostic::error($cursor, 1, format!("Expected {}", expected_token_types.join(" or "))))
        }
    }};
}

#[allow(unused_macros)]
macro_rules! expect_optional_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let result = expect_token!($parser, $cursor, $token_type);

        match result {
            Ok((token, cursor)) => Ok((Some(token), cursor)),
            Err(_) => Ok((None, $cursor)),
        }
    }};
}

macro_rules! expect_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let result = crate::parser::macros::expect_tokens!($parser, $cursor, $token_type);

        match result {
            Ok((tokens, cursor)) => {
                let token = tokens.first().unwrap().clone();
                if token.token_type == $token_type {
                    Ok((token, cursor))
                } else {
                    Err(Diagnostic::error($cursor, 1, "Invalid token"))
                }
            }
            Err(err) => Err(err),
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
        let mut is_valid = true;

        $(
            let lexeme = $parser.lexemes.get(i);

            match lexeme {
                Some(crate::scanner::lexeme::Lexeme::Valid(token)) => {
                    if token.token_type == $token_type {
                        tokens.push(token.clone());
                        i += 1;
                    } else {
                        is_valid = false;
                    }
                }
                _ => {
                    is_valid = false;
                }
            }
        )*

        if is_valid {
            Ok((tokens, i))
        } else {
            let tokens = vec![$($token_type.to_string()),*];
            Err(Diagnostic::error($cursor, tokens.len(), format!("Expected {}", tokens.join(" and "))))
        }
    }};
}

pub(crate) use expect_any_token;
pub(crate) use expect_expression;
#[allow(unused_imports)]
pub(crate) use expect_optional_token;
#[allow(unused_imports)]
pub(crate) use expect_statement;
pub(crate) use expect_token;
pub(crate) use expect_token_value;
pub(crate) use expect_tokens;
pub(crate) use expect_type;
pub(crate) use expect_valid_token;

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
            return Err(Diagnostic::error(
                &crate::scanner::lexeme::Range {
                    position: $cursor,
                    length: 0,
                },
                "Unexpected end of file",
            ));
        }

        let lexeme = lexeme.unwrap();

        match lexeme {
            Lexeme::Valid(token) => (token, lexeme.range()),
            Lexeme::Invalid(_) => return Err(Diagnostic::error(lexeme.range(), "Invalid token")),
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
            None => {
                let lexeme = $parser.lexemes.get($cursor).unwrap();
                Err(Diagnostic::error(lexeme.range(), format!("Expected `{}`", expected_token_types.join("` or `"))))
            }
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
        let result = crate::parser::macros::expect_tokens!($parser, $cursor, ($token_type));

        match result {
            Ok((lexemes, cursor)) => {
                let lexeme = lexemes.first().unwrap().clone();
                if let crate::scanner::lexeme::Lexeme::Valid(token) = &lexeme {
                    Ok((token, cursor))
                } else {
                    Err(Diagnostic::error(lexeme.range(), "Invalid token"))
                }
            }
            Err(err) => Err(err),
        }
    }};
}

macro_rules! expect_tokens {
    ($parser:expr, $cursor:expr, $(($($token_type:expr),*)),*) => {{
        let mut i = $cursor;
        let mut lexemes = Vec::new();
        let mut error: Option<Diagnostic> = None;

        $(
            if error.is_none() {
            match $parser.lexemes.get(i) {
                Some(lexeme) => {
                    if let crate::scanner::lexeme::Lexeme::Valid(token) = lexeme {
                        let mut matched = false;

                        $(
                            if $token_type == token.token_type {
                                matched = true;
                            } else {
                                error = Some(Diagnostic::error(lexeme.range(), format!("Expected `{}`", $token_type.to_string())));
                            }
                        )*

                        if matched {
                            lexemes.push(lexeme);
                            i += 1;
                        }
                    } else {
                        error = Some(Diagnostic::error(lexeme.range(), "Unknown token"));
                    }
                }
                None => {
                    let expected_token_types = vec![$($token_type.to_string()),*];
                    error = Some(Diagnostic::error(&crate::scanner::lexeme::Range {position: $cursor, length: 0}, format!("Expected `{}`", expected_token_types.join("` followed by `"))));
                }
            }
        }
        )*

        if let Some(err) = error {
            Err(err)
        } else {
            Ok((lexemes, i))
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
pub(crate) use expect_tokens;
pub(crate) use expect_type;
pub(crate) use expect_valid_token;

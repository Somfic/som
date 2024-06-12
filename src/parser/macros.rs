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

// allows for multiple token types to be expected
// peek_any_token!(parser, cursor, TokenType::Plus, TokenType::Minus);
macro_rules! expect_any_token {
    ($parser:expr, $cursor:expr, $($token_type:expr),*) => {{
        let expected_token_types = vec![$($token_type.to_string()),*];

        println!("Expecting any of: {:?}", expected_token_types);

        let expected_tokens = vec![$(expect_token!($parser, $cursor, $token_type)),*];

        println!("Expected tokens: {:?}", expected_tokens);

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

macro_rules! expect_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let result = crate::parser::macros::expect_tokens!($parser, $cursor, ($token_type));

        match result {
            Ok((lexemes, cursor)) => {
                let lexeme = lexemes.first().unwrap().clone();
                if let Lexeme::Valid(token) = &lexeme {
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
                    if let Lexeme::Valid(token) = lexeme {
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
pub(crate) use expect_statement;
pub(crate) use expect_token;
pub(crate) use expect_tokens;

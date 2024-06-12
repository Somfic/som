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

macro_rules! expect_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        let lexeme = crate::parser::macros::expect_tokens!($parser, $cursor, ($token_type))
            .map(|(lexemes, cursor)| (lexemes.first().unwrap().clone(), cursor))?;

        if let Lexeme::Valid(token) = &lexeme.0 {
            Ok((token, lexeme.1))
        } else {
            Err(Diagnostic::error(lexeme.0.range(), "Invalid token"))
        }
    }};
}

macro_rules! expect_tokens {
     ($parser:expr, $cursor:expr, $(($($token_type:expr),*)),*) => {{
            let mut i = $cursor;
            let mut lexemes = Vec::new();
             $(
                let lexeme = $parser.lexemes.get(i);

                if lexeme.is_none() {
                    let expected_token_types = vec![$($token_type.to_string()),*];
                    return Err(Diagnostic::error($parser.lexemes.last().unwrap().range(), format!("Expected `{}`", expected_token_types.join("` followed by `"))));
                }

                let lexeme = lexeme.unwrap();

                if let Lexeme::Valid(token) = lexeme {
                     $(
                        if $token_type != token.token_type {
                            return Err(Diagnostic::error(lexeme.range(), format!("Expected `{}`", $token_type.to_string())));
                        }
                    )*

                    lexemes.push(lexeme);
                    i += 1;
                } else {
                    return Err(Diagnostic::error(lexeme.range(), "Unknown token"));
                }
            )*

            // If all tokens matched, return the matched tokens
            Ok((lexemes, i))
        }};
}

pub(crate) use expect_expression;
pub(crate) use expect_statement;
pub(crate) use expect_token;
pub(crate) use expect_tokens;

macro_rules! expect_token {
    ($parser:expr, $cursor:expr, $token_type:expr) => {{
        crate::parser::macros::expect_tokens!($parser, $cursor, ($token_type))
            .map(|(lexemes, cursor)| (lexemes.first().unwrap().clone(), cursor))
    }};
}

macro_rules! expect_tokens {
     ($parser:expr, $cursor:expr, $(($($token_type:expr),*)),*) => {{
            let mut i = $cursor;
            let mut lexemes = Vec::new();
             $(
                let lexeme = $parser.lexemes.get(i);

                if lexeme.is_none() {
                    return Err(Diagnostic::error($parser.lexemes.last().unwrap().range(), "Unexpected end of input"));
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

pub(crate) use expect_token;
pub(crate) use expect_tokens;

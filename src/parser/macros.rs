macro_rules! expect_token {
    ($parser:ident, $token:ident) => {{
        use crate::diagnostic::Diagnostic;
        use crate::scanner::lexeme::TokenType;

        if let Some(TokenType::$token) = $parser.peek().map(|t| &t.token_type) {
            Ok($parser.consume().unwrap())
        } else {
            let token = $parser.peek().unwrap_or($parser.tokens.last().unwrap());
            let position = if $parser.peek().is_none() {
                token.range.position + token.range.length
            } else {
                token.range.position
            };

            Err(
                Diagnostic::error("expected_token", format!("Expected {}", TokenType::$token))
                    .with_snippet(crate::diagnostic::Snippet::primary(
                        token.range.file_id,
                        position,
                        1,
                        format!("Expected {} here", TokenType::$token),
                    ))
                    .with_note(format!(
                        "Expected {}, but got {} instead",
                        TokenType::$token,
                        token.token_type
                    )),
            )
        }
    }};

    ($parser:ident) => {
        if let Some(token) = $parser.peek() {
            Ok(token)
        } else {
            let token = $parser.tokens.last().unwrap();

            Err(
                Diagnostic::error("expected_token", "Unexpected end of file")
                    .with_snippet(crate::diagnostic::Snippet::primary(
                        token.range.file_id,
                        token.range.position + token.range.length,
                        1,
                        "Unexpected end of file",
                    ))
                    .with_note("Expected more code, but reached the end of the file"),
            )
        }
    };
}

macro_rules! expect_value {
    ($token:expr, $value:ident) => {{
        use crate::scanner::lexeme::TokenValue;

        match &$token.value {
            TokenValue::$value(value) => value,
            _ => panic!("expect_token_value! should only return identifiers"),
        }
    }};
}

pub(crate) use expect_token;
pub(crate) use expect_value;

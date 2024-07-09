macro_rules! expect_token {
    ($parser:ident, $token:ident) => {{
        use crate::diagnostic::Diagnostic;
        use crate::scanner::lexeme::TokenType;

        if let Some(TokenType::$token) = $parser.peek().map(|t| &t.token_type) {
            Ok($parser.consume().unwrap())
        } else {
            Err(Diagnostic::error(
                "expected_token",
                format!("Expected token {:?}", TokenType::$token),
            ))
        }
    }};
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

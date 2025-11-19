use crate::{
    ast::{
        BooleanType, CharacterType, DecimalType, FunctionType, I32Type, I64Type, StringType,
        StructType, Type,
    },
    lexer::TokenKind,
    parser::{Parse, Parser},
    ParserError, Result,
};

impl Parse for Type {
    type Params = ();

    fn parse(input: &mut Parser, _params: Self::Params) -> Result<Self> {
        let peek = input.peek_expect("a type")?.clone();

        let Some(parse_function) = input.lookup.type_lookup.get(&peek.kind).cloned() else {
            return ParserError::ExpectedType
                .to_diagnostic()
                .with_label(peek.span.clone().label("expected this to be a type"))
                .with_hint(format!("{} cannot be parsed as a type", peek))
                .to_err();
        };

        parse_function(input)
    }
}

impl Parse for I32Type {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(TokenKind::I32Type, "an integer", ParserError::ExpectedType)?;

        Ok(I32Type { span: token.span })
    }
}

impl Parse for I64Type {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(TokenKind::I64Type, "an integer", ParserError::ExpectedType)?;

        Ok(I64Type { span: token.span })
    }
}

impl Parse for DecimalType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(
            TokenKind::DecimalType,
            "a decimal",
            ParserError::ExpectedType,
        )?;

        Ok(DecimalType { span: token.span })
    }
}

impl Parse for StringType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(TokenKind::StringType, "a string", ParserError::ExpectedType)?;

        Ok(StringType { span: token.span })
    }
}

impl Parse for CharacterType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(
            TokenKind::CharacterType,
            "a character",
            ParserError::ExpectedType,
        )?;

        Ok(CharacterType { span: token.span })
    }
}

impl Parse for BooleanType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let token = input.expect(
            TokenKind::BooleanType,
            "a boolean",
            ParserError::ExpectedType,
        )?;

        Ok(BooleanType { span: token.span })
    }
}

impl Parse for FunctionType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        todo!()
    }
}

impl Parse for StructType {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(TokenKind::CurlyOpen, "a struct", ParserError::ExpectedType)?;

        let fields = vec![];

        while let Some(token) = input.peek() {
            if token.kind == TokenKind::CurlyClose {
                break;
            }

            if fields.len() > 0 {
                input.expect(TokenKind::Comma, "a comma between fields", )
            }

            let ident = input.parse_with()?;

            // there was no semicolon, this is the returning expression
            match statement {
                Statement::Expression(e) => expression = Some(e),
                s => ParserError::InvalidReturningExpression
                    .to_diagnostic()
                    .with_label(s.span().label("this statement"))
                    .with_hint(format!("{} cannot be used as a value", s))
                    .to_err()?,
            }
        }
    }
}

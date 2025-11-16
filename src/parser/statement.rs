use crate::{
    ast::{Expression, Scope, Statement},
    lexer::TokenKind,
    Parse, Parser, ParserError, Result, Untyped,
};

impl Parse for Statement<Untyped> {
    type Params = bool;

    fn parse(input: &mut Parser, with_semicolon: Self::Params) -> Result<Self> {
        let peek = input.peek_expect("a statement")?.clone();

        match input.lookup.statement_lookup.get(&peek.kind).cloned() {
            Some(statement_parser) => statement_parser(input),
            None => {
                match input.lookup.expression_lookup.get(&peek.kind) {
                    Some(_) => {
                        let expression = input.parse::<Expression<_>>()?;

                        if with_semicolon {
                            match input.expect(
                                TokenKind::Semicolon,
                                "a closing semicolon",
                                ParserError::ExpectedSemicolon,
                            ) {
                                Ok(_) => Ok(()),
                                Err(diagnostic) => Err(diagnostic
                                    .with_hint("statements must be closed with a semicolon")),
                            }?;
                        }

                        Ok(Statement::Expression(expression))
                    }
                    None => ParserError::ExpectedStatement
                        .to_diagnostic()
                        .with_label(peek.span.clone().label("expected this to be a statement"))
                        .with_hint(format!("{} cannot be parsed as a statement", peek))
                        .to_err(),
                }
            }
        }
    }
}

impl Parse for Scope<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        input.expect(
            TokenKind::CurlyOpen,
            "start of scope",
            ParserError::ExpectedScopeStart,
        )?;

        let mut statements = vec![];

        while let Some(token) = input.peek() {
            if token.kind == TokenKind::CurlyClose {
                break;
            }

            statements.push(input.parse::<Statement<_>>()?);
        }

        input.expect(
            TokenKind::CurlyClose,
            "end of scope",
            ParserError::ExpectedScopeEnd,
        )?;

        Ok(Scope { statements })
    }
}

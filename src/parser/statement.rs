use crate::{
    ast::{Declaration, Expression, Scope, Statement, StructType, Type, TypeDefinition},
    lexer::{Identifier, TokenKind},
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
        let open = input.expect(
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

        let close = input.expect(
            TokenKind::CurlyClose,
            "end of scope",
            ParserError::ExpectedScopeEnd,
        )?;

        Ok(Scope {
            statements,
            span: open.span + close.span,
        })
    }
}

impl Parse for Declaration<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
            TokenKind::Let,
            "a variable",
            ParserError::ExpectedDeclaration,
        )?;

        let name = input.parse()?;

        input.expect(TokenKind::Equal, "a value", ParserError::ExpectedValue)?;

        let value = input.parse::<Expression<_>>()?;

        Ok(Declaration {
            name,
            span: open.span + value.span().clone(),
            value: Box::new(value),
        })
    }
}

impl Parse for TypeDefinition {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
            TokenKind::Type,
            "a type definition",
            ParserError::ExpectedTypeDefinition,
        )?;

        let name = input.parse::<Identifier>()?;

        input.expect(TokenKind::Equal, "a type", ParserError::ExpectedType)?;

        let ty = match input.parse::<Type>()? {
            Type::Struct(s) => Type::struct_type(Some(name.clone()), s.fields, s.span),
            ty => ty,
        };

        Ok(TypeDefinition {
            span: open.span + ty.span().clone(),
            ty,
            name,
        })
    }
}

use crate::{
    ast::{Declaration, File, Visibility},
    lexer::TokenKind,
    Parse, Parser, ParserError, Result, Untyped,
};

impl Parse for File<Untyped> {
    type Params = ();

    fn parse(parser: &mut crate::parser::Parser, _params: Self::Params) -> Result<Self> {
        let mut declarations = Vec::new();

        while parser.peek().is_some() {
            let declaration = parser.parse()?;
            declarations.push(declaration);
        }

        Ok(File { declarations })
    }
}

impl Parse for Declaration<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let pub_peek = input.peek_expect("a declaration").cloned()?;

        let visibility = match pub_peek.kind {
            TokenKind::Pub => {
                input.next()?;

                if let Some(next) = input.peek() {
                    if next.kind == TokenKind::Mod {
                        input.next()?;
                        Visibility::Module
                    } else {
                        Visibility::Public
                    }
                } else {
                    Visibility::Public
                }
            }
            _ => Visibility::Private,
        };

        let peek = input.peek_expect("a declaration")?;
        let peek_kind = peek.kind.clone();
        let peek_span = peek.span.clone();
        let peek_display = format!("{}", peek);

        let declaration_parser = input.lookup.declaration_lookup.get(&peek_kind).cloned();

        let delcaration = match declaration_parser {
            Some(declaration_parser) => declaration_parser(input),
            None => ParserError::ExpectedDeclaration
                .to_diagnostic()
                .with_label(peek_span.label("expected this to be a declaration"))
                .with_hint(format!(
                    "{} cannot be parsed as a declaration",
                    peek_display
                ))
                .to_err(),
        }?;

        let close = input.expect(
            TokenKind::Semicolon,
            "a semicolon to close the declaration",
            ParserError::ExpectedSemicolon,
        )?;

        match delcaration {
            Declaration::ValueDefinition(mut declaration) => {
                declaration.visibility = visibility;
                Ok(Declaration::ValueDefinition(declaration))
            }
            Declaration::TypeDefinition(mut type_definition) => {
                type_definition.visibility = visibility;
                Ok(Declaration::TypeDefinition(type_definition))
            }
            declaration => match visibility {
                Visibility::Private => Ok(declaration),
                _ => ParserError::InvalidVisibilityModifier
                    .to_diagnostic()
                    .with_label(pub_peek.span.label("cannot be exported"))
                    .with_hint(format!("{} cannot be marked as exported", declaration))
                    .to_err(),
            },
        }
    }
}

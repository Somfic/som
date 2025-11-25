use crate::{
    ast::{
        Declaration, Expression, ExternDefinition, ExternFunction, FunctionDefinition, Import,
        Scope, Statement, StructType, Type, TypeDefinition, ValueDefinition, Visibility, WhileLoop,
    },
    lexer::{Identifier, Path, Token, TokenKind},
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

            statements.push(input.parse_with::<Statement<_>>(true)?);
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

impl Parse for ValueDefinition<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
            TokenKind::Let,
            "a value",
            ParserError::ExpectedValueDefinition,
        )?;

        let name = input.parse()?;

        input.expect(TokenKind::Equal, "a value", ParserError::ExpectedValue)?;

        let value = input.parse::<Expression<_>>()?;

        Ok(ValueDefinition {
            visibility: Visibility::Private,
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
            visibility: Visibility::Private,
            span: open.span + ty.span().clone(),
            ty,
            name,
        })
    }
}

impl Parse for ExternDefinition {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let start = input.expect(
            TokenKind::Extern,
            "an extern definition",
            ParserError::ExpectedExternDefinition,
        )?;

        let library = input.parse()?;

        fn parse_extern_function(input: &mut Parser) -> Result<ExternFunction> {
            let name = input.parse::<Identifier>()?;

            let alias = if let Some(token) = input.peek() {
                if token.kind == TokenKind::As {
                    input.expect(
                        TokenKind::As,
                        "an alias for the extern function",
                        ParserError::ExpectedExternFunctionAlias,
                    )?;
                    Some(input.parse::<Identifier>()?)
                } else {
                    None
                }
            } else {
                None
            };

            input.expect(
                TokenKind::Equal,
                "an extern function definition",
                ParserError::ExpectedExternFunctionDefinition,
            )?;

            let signature = match input.parse::<Type>()? {
                Type::Function(f) => f,
                _ => {
                    return ParserError::ExpectedFunctionType
                        .to_diagnostic()
                        .with_label(
                            input
                                .lexer
                                .cursor
                                .label("expected a function type for extern function"),
                        )
                        .to_err()
                }
            };

            let symbol = alias
                .map(|a| a.name)
                .unwrap_or_else(|| name.name.clone())
                .to_string();

            Ok(ExternFunction {
                name,
                symbol,
                span: signature.span.clone(),
                signature,
            })
        }

        match input.peek() {
            Some(token) if token.kind == TokenKind::CurlyOpen => {
                input.expect(
                    TokenKind::CurlyOpen,
                    "start of extern block",
                    ParserError::ExpectedScopeStart,
                )?;

                let mut functions = vec![];
                while let Some(token) = input.peek() {
                    if token.kind == TokenKind::CurlyClose {
                        break;
                    }

                    functions.push(parse_extern_function(input)?);
                    input.expect(
                        TokenKind::Semicolon,
                        "a closing semicolon",
                        ParserError::ExpectedSemicolon,
                    )?;
                }
                let end = input.expect(
                    TokenKind::CurlyClose,
                    "end of extern block",
                    ParserError::ExpectedScopeEnd,
                )?;
                Ok(ExternDefinition {
                    library,
                    functions,
                    span: start.span + end.span,
                })
            }
            _ => {
                let function = parse_extern_function(input)?;

                Ok(ExternDefinition {
                    library,
                    functions: vec![function],
                    span: start.span,
                })
            }
        }
    }
}

impl Parse for WhileLoop<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(TokenKind::While, "while loop", ParserError::ExpectedWhile)?;

        let condition = input.parse_with(crate::parser::lookup::Precedence::Ternary.as_u8())?;

        let statement = input.parse::<Statement<Untyped>>()?;

        Ok(WhileLoop {
            span: open.span + statement.span().clone(),
            condition,
            statement: Box::new(statement),
        })
    }
}

impl Parse for Import {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
            TokenKind::Use,
            "import statement",
            ParserError::ExpectedImport,
        )?;

        let module = input.parse::<Path>()?;

        Ok(Import {
            span: open.span + module.span.clone(),
            module,
        })
    }
}

impl Parse for FunctionDefinition<Untyped> {
    type Params = ();

    fn parse(input: &mut Parser, params: Self::Params) -> Result<Self> {
        let open = input.expect(
            TokenKind::Function,
            "a function definition",
            ParserError::ExpectedFunctionDefinition,
        )?;

        let name = input.parse::<Identifier>()?;

        // Parse parameter list: (param1 ~ Type1, param2 ~ Type2, ...)
        input.expect(
            TokenKind::ParenOpen,
            "a function parameter list",
            ParserError::ExpectedFunctionType,
        )?;

        let mut parameters = vec![];
        while let Some(token) = input.peek() {
            if token.kind == TokenKind::ParenClose {
                break;
            }

            // Parse parameter: name ~ type
            let param_name = input.parse::<Identifier>()?;
            input.expect(
                TokenKind::Tilde,
                "'~' before parameter type",
                ParserError::ExpectedTypeAnnotation,
            )?;
            let param_ty = input.parse::<Type>()?;

            parameters.push(crate::ast::Parameter {
                name: param_name,
                ty: param_ty,
            });

            if let Some(Token {
                kind: TokenKind::Comma,
                ..
            }) = input.peek()
            {
                input.next()?;
            } else {
                break;
            }
        }

        input.expect(
            TokenKind::ParenClose,
            "a function parameter list",
            ParserError::ExpectedFunctionType,
        )?;

        // Parse return type: -> Type (optional, defaults to unit)
        let returns = if input.peek().is_some_and(|t| t.kind == TokenKind::Arrow) {
            input.expect(
                TokenKind::Arrow,
                "a return type",
                ParserError::ExpectedFunctionType,
            )?;
            input.parse()?
        } else {
            // Default to unit type if no return type specified
            Type::Unit(crate::ast::UnitType {
                span: name.span.clone(),
            })
        };

        let body = input.parse::<Expression<Untyped>>()?;

        Ok(FunctionDefinition {
            id: input.next_function_id(),
            visibility: Visibility::Private,
            name,
            parameters,
            returns,
            span: open.span + body.span().clone(),
            body,
        })
    }
}

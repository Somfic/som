use crate::{Lifetime, Type, lexer::TokenKind, parser::Parser};

impl<'src> Parser<'src, '_> {
    /// Parse a type annotation
    pub fn parse_type(&mut self) -> Option<Type> {
        // Check for reference type: &T or &mut T or &'a T
        if self.at(TokenKind::Ampersand) {
            return self.parse_reference_type();
        }

        // Check for function type: fn(A, B) -> C
        if self.at(TokenKind::Fn) {
            return self.parse_fn_type();
        }

        // Parse primary type
        self.parse_primary_type()
    }

    /// Parse a reference type: &T, &mut T, &'a T, &'a mut T
    fn parse_reference_type(&mut self) -> Option<Type> {
        self.expect(TokenKind::Ampersand)?;

        // Parse optional lifetime: 'a
        let lifetime = if self.at(TokenKind::SingleQuote) {
            self.parse_lifetime()?
        } else {
            Lifetime::Unspecified
        };

        // Parse optional mut
        let mutable = self.eat(TokenKind::Mut);

        // Parse inner type
        let to = Box::new(self.parse_type()?);

        Some(Type::Reference {
            mutable,
            lifetime,
            to,
        })
    }

    /// Parse a lifetime: 'a, 'static
    fn parse_lifetime(&mut self) -> Option<Lifetime> {
        self.expect(TokenKind::SingleQuote)?;

        if self.at(TokenKind::Ident) {
            let text = self.peek_token().text;
            self.advance();

            if text == "static" {
                Some(Lifetime::Static)
            } else {
                Some(Lifetime::Named(text.into()))
            }
        } else {
            self.error("expected lifetime name".into());
            None
        }
    }

    /// Parse a function type: fn(A, B) -> C
    fn parse_fn_type(&mut self) -> Option<Type> {
        self.expect(TokenKind::Fn)?;
        self.expect(TokenKind::OpenParen)?;

        let mut arguments = Vec::new();
        if !self.at(TokenKind::CloseParen) {
            arguments.push(self.parse_type()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break; // Trailing comma
                }
                arguments.push(self.parse_type()?);
            }
        }

        self.expect(TokenKind::CloseParen)?;

        // Parse return type (required for fn types, defaults to Unit)
        let returns = if self.eat(TokenKind::Arrow) {
            Box::new(self.parse_type()?)
        } else {
            Box::new(Type::Unit)
        };

        Some(Type::Fun { arguments, returns })
    }

    /// Parse a primary (non-compound) type
    fn parse_primary_type(&mut self) -> Option<Type> {
        match self.peek() {
            // Built-in types
            TokenKind::I32 => {
                self.advance();
                Some(Type::I32)
            }
            TokenKind::Bool => {
                self.advance();
                Some(Type::Bool)
            }
            TokenKind::Str => {
                self.advance();
                Some(Type::Str)
            }

            // Other integer types (map to I32 for now, can be expanded)
            TokenKind::I8
            | TokenKind::I16
            | TokenKind::I64
            | TokenKind::I128
            | TokenKind::ISize => {
                let text = self.peek_token().text;
                self.advance();
                Some(Type::Named(text.into()))
            }

            // u8 is a built-in type
            TokenKind::U8 => {
                self.advance();
                Some(Type::U8)
            }

            // Other unsigned integer types (not yet fully supported)
            TokenKind::U16
            | TokenKind::U32
            | TokenKind::U64
            | TokenKind::U128
            | TokenKind::USize => {
                let text = self.peek_token().text;
                self.advance();
                Some(Type::Named(text.into()))
            }

            // Float types
            TokenKind::F32 => {
                self.advance();
                Some(Type::F32)
            }
            TokenKind::F64 => {
                let text = self.peek_token().text;
                self.advance();
                Some(Type::Named(text.into()))
            }

            TokenKind::Star => {
                self.advance();
                Some(Type::Pointer)
            }

            // Char type
            TokenKind::Char => {
                let text = self.peek_token().text;
                self.advance();
                Some(Type::Named(text.into()))
            }

            // Named/user-defined type
            TokenKind::Ident => {
                let text = self.peek_token().text;
                self.advance();
                Some(Type::Named(text.into()))
            }

            // Parenthesized type or unit
            TokenKind::OpenParen => {
                self.advance();
                if self.eat(TokenKind::CloseParen) {
                    // () is Unit type
                    Some(Type::Unit)
                } else {
                    // (Type) - parenthesized type
                    let inner = self.parse_type()?;
                    self.expect(TokenKind::CloseParen)?;
                    Some(inner)
                }
            }

            _ => {
                self.error_expected(&[
                    TokenKind::I32,
                    TokenKind::Bool,
                    TokenKind::Str,
                    TokenKind::Ident,
                    TokenKind::Ampersand,
                    TokenKind::Fn,
                ]);
                None
            }
        }
    }
}

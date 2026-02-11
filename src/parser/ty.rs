use crate::{Lifetime, Type, lexer::TokenKind, parser::Parser};

impl<'src> Parser<'src> {
    pub fn parse_type(&mut self) -> Option<Type> {
        let kind = self.peek();
        match kind {
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
            TokenKind::Ampersand => {
                self.advance(); // consume &
                let mutable = self.eat(TokenKind::Mut);

                let lifetime = if self.eat(TokenKind::SingleQuote) {
                    let (name, _) = self.parse_ident()?;
                    if &*name.value == "static" {
                        Lifetime::Static
                    } else {
                        Lifetime::Named(name.value)
                    }
                } else {
                    Lifetime::Unspecified
                };

                let inner_type = self.parse_type()?;
                Some(Type::Reference {
                    mutable,
                    lifetime,
                    to: Box::new(inner_type),
                })
            }
            TokenKind::Ident => {
                let (name, _) = self.parse_ident()?;
                Some(Type::Named(name.value))
            }
            _ => {
                self.error(vec![TokenKind::I32, TokenKind::Bool, TokenKind::Str, TokenKind::Ident]);
                None
            }
        }
    }
}

use som_ast::{Id, Primitive, TypeExpr};
use som_lexer::TokenKind;

use crate::Parser;

impl Parser<'_> {
    /// Parses a type annotation.
    pub(crate) fn parse_type(&mut self) -> Option<Id<TypeExpr>> {
        let start = self.current_span();

        let ty = match self.peek() {
            TokenKind::Ampersand => {
                self.advance();
                let lifetime = if self.eat(TokenKind::SingleQuote) {
                    Some(self.parse_ident("a lifetime name")?)
                } else {
                    None
                };
                let mutable = self.eat(TokenKind::Mut);
                let to = self.parse_type()?;
                TypeExpr::Reference {
                    mutable,
                    lifetime,
                    to,
                }
            }

            TokenKind::Fn => {
                self.advance();
                self.expect(TokenKind::OpenParen, "function parameter types")?;
                let params = self.parse_comma_list(TokenKind::CloseParen, |p| p.parse_type())?;
                self.expect(
                    TokenKind::CloseParen,
                    "a closing parenthesis after parameter types",
                )?;
                let returns = if self.eat(TokenKind::Arrow) {
                    Some(self.parse_type()?)
                } else {
                    None
                };
                TypeExpr::Fn { params, returns }
            }

            TokenKind::Star => {
                self.advance();
                TypeExpr::Pointer
            }

            TokenKind::Ident => TypeExpr::Named(self.parse_path("a type name")?),

            TokenKind::OpenParen => {
                self.advance();
                if self.eat(TokenKind::CloseParen) {
                    TypeExpr::Unit
                } else {
                    // A parenthesized type is just the inner type.
                    let inner = self.parse_type()?;
                    self.expect(TokenKind::CloseParen, "a closing parenthesis for the type")?;
                    return Some(inner);
                }
            }

            kind => match primitive(kind) {
                Some(primitive) => {
                    self.advance();
                    TypeExpr::Primitive(primitive)
                }
                None => {
                    self.error_expected(
                        &[
                            TokenKind::I32,
                            TokenKind::Bool,
                            TokenKind::Str,
                            TokenKind::Ident,
                            TokenKind::Ampersand,
                            TokenKind::Fn,
                        ],
                        "a type",
                    );
                    return None;
                }
            },
        };

        let span = self.span_from(&start);
        Some(self.ast.types.alloc(ty, span))
    }
}

fn primitive(kind: TokenKind) -> Option<Primitive> {
    Some(match kind {
        TokenKind::I8 => Primitive::I8,
        TokenKind::I16 => Primitive::I16,
        TokenKind::I32 => Primitive::I32,
        TokenKind::I64 => Primitive::I64,
        TokenKind::I128 => Primitive::I128,
        TokenKind::ISize => Primitive::ISize,
        TokenKind::U8 => Primitive::U8,
        TokenKind::U16 => Primitive::U16,
        TokenKind::U32 => Primitive::U32,
        TokenKind::U64 => Primitive::U64,
        TokenKind::U128 => Primitive::U128,
        TokenKind::USize => Primitive::USize,
        TokenKind::F32 => Primitive::F32,
        TokenKind::F64 => Primitive::F64,
        TokenKind::Bool => Primitive::Bool,
        TokenKind::Char => Primitive::Char,
        TokenKind::Str => Primitive::Str,
        _ => return None,
    })
}

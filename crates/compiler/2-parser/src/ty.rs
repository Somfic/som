use som_ast::{BinOp, Id, Primitive, Span, TypeExpr};
use som_lexer::TokenKind;

use crate::Parser;

impl Parser<'_> {
    /// Parses a type annotation. A `?` after it makes the whole type optional: `&i32?` is
    /// an optional reference, and `&(i32?)` a reference to an optional. A `!` after that
    /// makes it failable: `User?!` fails, or gives a `User` or `none`. `!` alone is `()!`.
    pub(crate) fn parse_type(&mut self) -> Option<Id<TypeExpr>> {
        let start = self.current_span();
        let mut ty = match self.at(TokenKind::Bang) {
            true => self.ast.types.alloc(TypeExpr::Unit, start.clone()),
            false => self.parse_type_without_optional()?,
        };
        while self.eat(TokenKind::Question) {
            let span = self.span_from(&start);
            ty = self.ast.types.alloc(TypeExpr::Optional(ty), span);
        }
        if self.eat(TokenKind::Bang) {
            let errors = self.parse_errors()?;
            let span = self.span_from(&start);
            ty = self.ast.types.alloc(TypeExpr::Failable { ok: ty, errors }, span);
        }
        Some(ty)
    }

    /// What follows the `!` of `T!`: the type it fails with, `T!ParseError`, several,
    /// `T!(IoError, ParseError)`, or nothing, for them to be worked out.
    fn parse_errors(&mut self) -> Option<Option<Vec<Id<TypeExpr>>>> {
        match self.peek() {
            TokenKind::Ident => Some(Some(vec![self.parse_type_without_optional()?])),
            TokenKind::OpenParen => {
                self.advance();
                let errors = self.parse_comma_list(TokenKind::CloseParen, |p| {
                    p.parse_type_without_optional()
                })?;
                self.expect(TokenKind::CloseParen, "`)` after the types it fails with")?;
                Some(Some(errors))
            }
            _ => Some(None),
        }
    }

    pub(crate) fn at_primitive_type(&self) -> bool {
        matches!(self.peek(), TokenKind::Primitive(_))
    }

    /// `<A, B>` after a struct's name in a type, if it's there. An argument is a type, or a
    /// value: `Buffer<64>`, `Socket<true>`.
    pub(crate) fn parse_type_args(&mut self) -> Option<Vec<Id<TypeExpr>>> {
        if !self.eat(TokenKind::LessThan) {
            return Some(Vec::new());
        }
        let args = self.parse_comma_list(TokenKind::GreaterThan, |p| match p.peek() {
            TokenKind::Int | TokenKind::True | TokenKind::False => {
                p.parse_value_expr("a type argument")
            }
            // `N`, a type or a value parameter, or the start of arithmetic on one: `N + 1`.
            TokenKind::Ident => {
                let start = p.current_span();
                let first = p.parse_type()?;
                p.parse_value_rest(first, &start)
            }
            _ => p.parse_type(),
        })?;
        self.expect(
            TokenKind::GreaterThan,
            "a closing angle bracket for type arguments",
        )?;
        Some(args)
    }

    /// A length, as in `[T; N]` and `[value; N]`: a number, a value parameter, or `+` and `*`
    /// of those, `[T; R * C]`.
    pub(crate) fn parse_length(&mut self, what: &str) -> Option<Id<TypeExpr>> {
        self.parse_value_expr(what)
    }

    /// A value written where a type argument goes: `64`, `true`, `N`, or a sum of products of
    /// those, `2 * N + 1`. `*` binds tighter than `+`, and parentheses group.
    fn parse_value_expr(&mut self, what: &str) -> Option<Id<TypeExpr>> {
        let start = self.current_span();
        let first = self.parse_value_atom(what)?;
        self.parse_value_rest(first, &start)
    }

    /// After `first`, which started at `start`: `+ 1` after `N`.
    fn parse_value_rest(&mut self, first: Id<TypeExpr>, start: &Span) -> Option<Id<TypeExpr>> {
        let mut sum = self.parse_value_product(first, start)?;
        while self.eat(TokenKind::Plus) {
            let term_start = self.current_span();
            let atom = self.parse_value_atom("a value to add")?;
            let term = self.parse_value_product(atom, &term_start)?;
            let span = self.span_from(start);
            let binary = TypeExpr::Binary {
                op: BinOp::Add,
                lhs: sum,
                rhs: term,
            };
            sum = self.ast.types.alloc(binary, span);
        }
        Some(sum)
    }

    fn parse_value_product(&mut self, first: Id<TypeExpr>, start: &Span) -> Option<Id<TypeExpr>> {
        let mut product = first;
        while self.eat(TokenKind::Star) {
            let factor = self.parse_value_atom("a value to multiply by")?;
            let span = self.span_from(start);
            let binary = TypeExpr::Binary {
                op: BinOp::Multiply,
                lhs: product,
                rhs: factor,
            };
            product = self.ast.types.alloc(binary, span);
        }
        Some(product)
    }

    fn parse_value_atom(&mut self, what: &str) -> Option<Id<TypeExpr>> {
        let start = self.current_span();
        let value = match self.peek() {
            TokenKind::True | TokenKind::False => {
                let value = self.at(TokenKind::True);
                self.advance();
                TypeExpr::Bool(value)
            }
            TokenKind::Ident => TypeExpr::Named {
                path: self.parse_path(what)?,
                args: Vec::new(),
            },
            TokenKind::OpenParen => {
                self.advance();
                let inner = self.parse_value_expr(what)?;
                self.expect(TokenKind::CloseParen, "a closing parenthesis")?;
                return Some(inner);
            }
            _ => TypeExpr::Int(self.parse_count(what)?),
        };
        let span = self.span_from(&start);
        Some(self.ast.types.alloc(value, span))
    }

    /// Whether the `[` here starts a slice type, `[T]`, rather than an array type, `[T; N]`:
    /// whether no `;` comes before its `]`. Only looks ahead.
    fn slice_ahead(&self) -> bool {
        let mut depth = 0;
        for n in 0.. {
            match self.peek_nth(n) {
                TokenKind::OpenBracket => depth += 1,
                TokenKind::CloseBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return true;
                    }
                }
                TokenKind::Semicolon if depth == 1 => return false,
                TokenKind::Eof => return true,
                _ => {}
            }
        }
        unreachable!("the token stream ends in `Eof`")
    }

    fn parse_type_without_optional(&mut self) -> Option<Id<TypeExpr>> {
        let start = self.current_span();
        let ty = match self.peek() {
            TokenKind::Ampersand => self.parse_reference_type()?,
            TokenKind::Fn => self.parse_fn_type()?,
            TokenKind::Star => self.parse_pointer_type()?,
            // `#T`: shared by its copies, counted.
            TokenKind::Hash => {
                self.advance();
                TypeExpr::Shared(self.parse_type_without_optional()?)
            }
            TokenKind::OpenBracket => self.parse_bracket_type(&start)?,
            // `dyn Display`: any type implementing it.
            TokenKind::Dyn => {
                self.advance();
                let path = self.parse_path("a trait's name")?;
                let args = self.parse_type_args()?;
                TypeExpr::Dyn { path, args }
            }
            TokenKind::Ident | TokenKind::SelfType => {
                let path = self.parse_path("a type name")?;
                let args = self.parse_type_args()?;
                TypeExpr::Named { path, args }
            }
            TokenKind::OpenParen => {
                self.advance();
                if !self.eat(TokenKind::CloseParen) {
                    let inner = self.parse_type()?;
                    self.expect(TokenKind::CloseParen, "a closing parenthesis for the type")?;
                    return Some(inner);
                }
                TypeExpr::Unit
            }
            kind => match primitive(kind) {
                Some(primitive) => {
                    self.advance();
                    TypeExpr::Primitive(primitive)
                }
                None => {
                    self.error_expected(EXPECTED_TYPE, "a type");
                    return None;
                }
            },
        };
        let span = self.span_from(&start);
        Some(self.ast.types.alloc(ty, span))
    }

    /// `&T`, `&mut T`, `&'a T`, or a slice: `&[T]`, `&mut [T]`. `&[T; N]` is a reference to
    /// an array.
    fn parse_reference_type(&mut self) -> Option<TypeExpr> {
        self.advance();
        let lifetime = if self.eat(TokenKind::SingleQuote) {
            Some(self.parse_ident("a lifetime name")?)
        } else {
            None
        };
        let mutable = self.eat(TokenKind::Mut);
        if lifetime.is_some() || !self.at(TokenKind::OpenBracket) || !self.slice_ahead() {
            // A `?` after it is for the reference, not what it points to.
            let to = self.parse_type_without_optional()?;
            return Some(TypeExpr::Reference {
                mutable,
                lifetime,
                to,
            });
        }
        self.advance();
        let inner = self.current_span();
        if self.eat(TokenKind::Mut) {
            self.error_at(
                inner,
                "`mut` goes before the brackets",
                "a slice that can change its elements is `&mut [T]`",
            );
        }
        let element = self.parse_type()?;
        self.expect(
            TokenKind::CloseBracket,
            "a closing bracket after the slice type",
        )?;
        Some(TypeExpr::Slice { element, mutable })
    }

    fn parse_fn_type(&mut self) -> Option<TypeExpr> {
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
        Some(TypeExpr::Fn { params, returns })
    }

    /// `*`, or `*T` when a type follows.
    fn parse_pointer_type(&mut self) -> Option<TypeExpr> {
        self.advance();
        let pointee = matches!(
            self.peek(),
            TokenKind::Ident
                | TokenKind::SelfType
                | TokenKind::Dyn
                | TokenKind::Star
                | TokenKind::Ampersand
                | TokenKind::OpenBracket
                | TokenKind::Primitive(_)
        );
        match pointee {
            true => Some(TypeExpr::PointerTo(self.parse_type_without_optional()?)),
            false => Some(TypeExpr::Pointer),
        }
    }

    /// `[T; N]`. A slice is borrowed, `&[T]`: without the `&` it's reported, and read as one.
    fn parse_bracket_type(&mut self, start: &Span) -> Option<TypeExpr> {
        self.advance();
        let mutable = self.eat(TokenKind::Mut);
        let element = self.parse_type()?;
        if !mutable && !self.at(TokenKind::CloseBracket) {
            self.expect(TokenKind::Semicolon, "a `;` before the array's length")?;
            let len = self.parse_length("the array's length")?;
            self.expect(
                TokenKind::CloseBracket,
                "a closing bracket after the array type",
            )?;
            return Some(TypeExpr::Array { element, len });
        }
        self.expect(
            TokenKind::CloseBracket,
            "a closing bracket after the slice type",
        )?;
        // `impl Index<usize> for [T]`: the elements, which `&self` lends.
        if self.ctx.impl_target && !mutable {
            return Some(TypeExpr::Elements(element));
        }
        let written = if mutable { "&mut [T]" } else { "&[T]" };
        self.error_at(
            self.span_from(start),
            "a slice is borrowed",
            &format!("write it `{written}`"),
        );
        Some(TypeExpr::Slice { element, mutable })
    }
}

const EXPECTED_TYPE: &[TokenKind] = &[
    TokenKind::Primitive(Primitive::I32),
    TokenKind::Primitive(Primitive::Bool),
    TokenKind::Primitive(Primitive::Str),
    TokenKind::Ident,
    TokenKind::Ampersand,
    TokenKind::Fn,
];

pub(crate) fn primitive(kind: TokenKind) -> Option<Primitive> {
    match kind {
        TokenKind::Primitive(primitive) => Some(primitive),
        _ => None,
    }
}

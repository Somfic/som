use som_ast::{AssocType, Const, Expr, Func, Generic, Id, Ident, Impl, Path, Trait, TypeExpr};
use som_base::message::code;
use som_base::msg;
use som_lang::Builtin;
use som_lexer::TokenKind;

use super::signature;
use crate::Parser;
use crate::report::RecoveryLevel;

type TraitRef = (Path, Vec<Id<TypeExpr>>);

/// What an `impl` block holds.
#[derive(Default)]
struct Members {
    methods: Vec<Id<Func>>,
    types: Vec<(Ident, Id<TypeExpr>)>,
    consts: Vec<Id<Const>>,
}

impl Parser<'_> {
    /// `impl<T> Pair<T> { … }`, or `impl<T> Iterator for ListIter<T> { type Item = &T; … }`.
    pub(super) fn parse_impl(&mut self) -> Option<Impl> {
        self.expect(TokenKind::Impl, "an impl block")?;
        let generics = self.parse_generics(false)?;
        // `impl i32 { … }`: methods for a built-in type.
        if !self.at(TokenKind::Ident) {
            let ty = self.parse_type()?;
            let target = self.type_as_path(ty);
            return self.parse_impl_body(generics, None, target, Vec::new(), Some(ty));
        }
        let first = self.parse_path("a struct or trait name")?;
        let first_args = self.parse_type_args()?;
        if !self.eat(TokenKind::For) {
            return self.parse_impl_body(generics, None, first, first_args, None);
        }
        // `Iterator for ListIter<T>`: what came first was the trait. `T?` is a type, not a
        // struct's name.
        let trait_ = Some((first, first_args));
        if self.at(TokenKind::Ident) && self.peek_nth(1) != TokenKind::Question {
            let target = self.parse_path("a struct name")?;
            let target_args = self.parse_type_args()?;
            return self.parse_impl_body(generics, trait_, target, target_args, None);
        }
        // A type that isn't a struct: `bool`, `[T; N]`, `&T`, a slice's `[T]`.
        let ty = self.with_flag(|c| &mut c.impl_target, true, |p| p.parse_type())?;
        let target = self.type_as_path(ty);
        self.parse_impl_body(generics, trait_, target, Vec::new(), Some(ty))
    }

    /// A type that isn't a struct, named by its text, for showing.
    fn type_as_path(&self, ty: Id<TypeExpr>) -> Path {
        let span = self.ast.types.span(ty).clone();
        Path {
            segments: vec![Ident {
                name: span.get_text().as_ref().into(),
                span,
            }],
        }
    }

    /// An impl block's `{ … }`, after its header.
    fn parse_impl_body(
        &mut self,
        generics: Vec<Generic>,
        trait_: Option<TraitRef>,
        target: Path,
        target_args: Vec<Id<TypeExpr>>,
        target_ty: Option<Id<TypeExpr>>,
    ) -> Option<Impl> {
        self.expect(TokenKind::OpenBrace, "an impl block")?;
        let mut members = Members::default();
        self.ctx.impl_block = true;
        let outside = self.when.conditions.len();
        while !self.at_eof() {
            // The end of an `@if` among the members, or of the block.
            if self.at(TokenKind::CloseBrace) {
                if self.when.conditions.len() == outside {
                    break;
                }
                self.close_member_if()?;
                continue;
            }
            // `@if N >= 3 { pub prop z = … }`: members only some of the types have.
            if self.at_builtin(Builtin::If) {
                self.advance();
                let condition = self.parse_settled_condition()?;
                self.expect(TokenKind::OpenBrace, "`{` and the members")?;
                self.when.conditions.push(condition);
                self.when.frames.push((1, true));
                continue;
            }
            if self.at_item_for(true) {
                self.expand_for();
                continue;
            }
            if self.at_where() {
                self.parse_wheres();
                continue;
            }
            self.visibility = self.parse_visibility();
            self.parse_impl_member(&mut members, &generics)?;
        }
        self.ctx.impl_block = false;
        self.expect(TokenKind::CloseBrace, "a closing brace after impl block")?;
        let close = self.previous_span();
        // Every method is generic over the impl's parameters, ahead of its own.
        for &method in &members.methods {
            let own = std::mem::take(&mut self.ast.funcs.get_mut(method).generics);
            self.ast.funcs.get_mut(method).generics = generics.iter().cloned().chain(own).collect();
        }
        Some(Impl {
            target,
            target_args,
            target_ty,
            trait_,
            types: members.types,
            generics,
            methods: members.methods,
            consts: members.consts,
            close,
        })
    }

    /// The `}` of an `@if` among the members, at it, and an `else` or `else @if` after it.
    fn close_member_if(&mut self) -> Option<()> {
        self.advance();
        let (count, conditional) = self.when.frames.pop().unwrap_or((0, false));
        let kept = self.when.conditions.len() - count;
        let mut frame: Vec<_> = self.when.conditions.drain(kept..).collect();
        if !self.at(TokenKind::Else) {
            return Some(());
        }
        let else_span = self.current_span();
        self.advance();
        if !conditional {
            self.error_at(
                else_span,
                msg!["an ", code("else"), " after an ", code("else")],
                msg!["nothing is left for it"],
            );
        }
        // `else`: where the conditions before it don't hold.
        if conditional && let Some(last) = frame.pop() {
            let span = self.ast.exprs.span(last).clone();
            frame.push(self.ast.exprs.alloc(Expr::Not(last), span));
        }
        // `else @if M >= 3 {`: and where this one does.
        let conditional = self.at_builtin(Builtin::If);
        if conditional {
            self.advance();
            frame.push(self.parse_settled_condition()?);
        }
        self.expect(TokenKind::OpenBrace, "`{` and the members")?;
        self.when.frames.push((frame.len(), conditional));
        self.when.conditions.extend(frame);
        Some(())
    }

    /// A method, property, associated type or constant, after who can see it.
    fn parse_impl_member(&mut self, members: &mut Members, generics: &[Generic]) -> Option<()> {
        // `type Item = &T;`
        if self.eat(TokenKind::Type) {
            let Some(name) = self.parse_ident("an associated type name") else {
                self.recover(RecoveryLevel::Member);
                return Some(());
            };
            self.expect(TokenKind::Equals, "`=` and the type")?;
            let ty = self.parse_type()?;
            self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
            members.types.push((name, ty));
        } else if self.at_const(0) {
            match self.parse_const() {
                Some(id) => {
                    self.check_impl_const(id, generics);
                    members.consts.push(id);
                }
                None => self.recover(RecoveryLevel::Member),
            }
        } else if self.at_fn() || self.at(TokenKind::Prop) || self.c_style_fn(0) {
            match self.parse_function() {
                Some(id) => members.methods.push(id),
                None => self.recover(RecoveryLevel::Member),
            }
        } else {
            self.error_missing("expected method declaration", "a method declaration");
            self.recover(RecoveryLevel::Member);
        }
        Some(())
    }

    /// A constant's type and value can't depend on the block's parameters: it's one value,
    /// not one per type the block is for.
    fn check_impl_const(&mut self, id: Id<Const>, generics: &[Generic]) {
        let Some(first) = generics.first() else {
            return;
        };
        let span = self.ast.consts[id].name.span.clone();
        self.error(
            span,
            "a constant can't be in an impl block with type parameters",
            msg![
                "it would be one value for every ",
                code(first.name.name.to_string()),
                "; put it outside the block, as a module's constant"
            ],
            None,
        );
    }

    /// `trait Iterator { type Item; fn next(self: &mut Self) -> Item?; }`. A method without a
    /// body ends in `;`.
    pub(super) fn parse_trait(&mut self) -> Option<Id<Trait>> {
        let doc = self.doc_comment();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        self.expect(TokenKind::Trait, "a trait declaration")?;
        let name = self.parse_ident("a trait name")?;
        let generics = self.parse_generics(true)?;
        // `trait IndexMut: Index`
        let mut supers = Vec::new();
        if self.eat(TokenKind::Colon) {
            loop {
                supers.push(self.parse_path("a trait name")?);
                if !self.eat(TokenKind::Plus) {
                    break;
                }
            }
        }
        self.expect(TokenKind::OpenBrace, "the trait's methods")?;
        self.trait_supers = supers.clone();
        let mut types = Vec::new();
        let mut methods = Vec::new();
        self.ctx.impl_block = true;
        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            if self.at(TokenKind::Type) {
                let doc = self.doc_comment();
                self.advance();
                let Some(name) = self.parse_ident("an associated type name") else {
                    self.recover(RecoveryLevel::Member);
                    continue;
                };
                self.expect(TokenKind::Semicolon, "a semicolon after the type")?;
                types.push(AssocType { doc, name });
            } else if self.at_where() {
                self.parse_wheres();
            } else if self.at_fn() || self.at(TokenKind::Prop) {
                match self.parse_trait_method(&name, &generics) {
                    Some(id) => methods.push(id),
                    None => self.recover(RecoveryLevel::Member),
                }
            } else {
                self.error_missing(
                    "expected a method or `type` declaration",
                    "a method or associated type",
                );
                self.recover(RecoveryLevel::Member);
            }
        }
        self.ctx.impl_block = false;
        self.expect(TokenKind::CloseBrace, "a closing brace after the trait")?;
        let span = self.span_from(&start);
        let t = Trait {
            doc,
            visibility,
            name,
            generics,
            types,
            supers,
            methods,
        };
        Some(self.ast.traits.alloc(t, span))
    }

    /// A trait's method: a signature ending in `;`, or with a body. It's generic over `Self`,
    /// the type implementing it, then the trait's generics, then its own.
    fn parse_trait_method(
        &mut self,
        trait_name: &Ident,
        trait_generics: &[Generic],
    ) -> Option<Id<Func>> {
        let (doc, requires, listens) = self.take_wheres();
        let start = self.current_span();
        let prop = self.eat(TokenKind::Prop);
        if !prop {
            self.expect_fn("a method declaration")?;
        }
        let name = self.parse_ident("a method name")?;
        let own = self.parse_generics(false)?;
        let (params, variadic) = match prop {
            true => (self.prop_params(&name)?, false),
            false => self.parse_params(false)?,
        };
        let returns = if self.void_return() {
            None
        } else if self.eat(TokenKind::Arrow) || prop && self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = match self.eat(TokenKind::Semicolon) {
            true => None,
            false => Some(self.parse_body("method")?),
        };
        let this = Generic {
            name: Ident {
                name: som_lang::SELF_TYPE.into(),
                span: trait_name.span.clone(),
            },
            ty: None,
            default: None,
            bounds: std::iter::once(Path {
                segments: vec![trait_name.clone()],
            })
            .chain(self.trait_supers.iter().cloned())
            .collect(),
        };
        // The defaults are the trait's: a method's copies don't have them.
        let generics = std::iter::once(this)
            .chain(trait_generics.iter().map(|g| Generic {
                default: None,
                ..g.clone()
            }))
            .chain(own)
            .collect();
        let span = self.span_from(&start);
        let func = Func {
            generics,
            params,
            returns,
            body,
            variadic,
            in_trait: true,
            prop,
            requires,
            listens,
            when: self.when.conditions.clone(),
            // As visible as the trait.
            ..signature(doc, som_ast::Visibility::Public, name)
        };
        Some(self.ast.funcs.alloc(func, span))
    }
}

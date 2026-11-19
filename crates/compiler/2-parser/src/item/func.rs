use som_ast::{Bound, Expr, Func, Generic, Id, Ident, Param, Path, Span, TypeExpr};
use som_base::message::code;
use som_base::{Fix, msg};
use som_lexer::TokenKind;

use super::signature;
use crate::Parser;
use crate::ty::primitive;

impl Parser<'_> {
    /// `fn name<T>(params) -> Ret { body }` or `fn name<T>(params) -> Ret = value;`, or in an
    /// impl block, `prop name: Ret { body }` or `prop name: Ret = value;`.
    pub(super) fn parse_function(&mut self) -> Option<Id<Func>> {
        let super::attrs::Attrs {
            doc,
            requires,
            listens,
            test,
        } = self.take_wheres();
        let visibility = std::mem::take(&mut self.visibility);
        let start = self.current_span();
        let prop = self.eat(TokenKind::Prop);
        // `int sum()`: another language's function, its return type first.
        let c_return = if !prop && self.c_style_fn(0) {
            let ty_start = self.current_span();
            let ty = self.parse_type()?;
            Some((ty, self.span_from(&ty_start)))
        } else {
            None
        };
        if !prop && c_return.is_none() {
            self.expect_fn("a function declaration")?;
        }
        let name = self.parse_ident("a function name")?;
        let generics = self.parse_generics(false)?;
        let (params, variadic) = match prop {
            true => (self.prop_params(&name)?, false),
            false => self.parse_params(false)?,
        };
        let returns = match c_return {
            Some((ty, ty_span)) => {
                self.report_c_style_fn(&name, ty_span);
                Some(ty)
            }
            None => self.parse_returns(prop, &name)?,
        };
        let body = Some(self.parse_body(if prop { "property" } else { "function" })?);
        let span = self.span_from(&start);
        let func = Func {
            generics,
            params,
            returns,
            body,
            variadic,
            prop,
            requires,
            listens,
            test,
            when: self.when.conditions.clone(),
            ..signature(doc, visibility, name)
        };
        Some(self.ast.funcs.alloc(func, span))
    }

    /// `int sum(…)`, its return type at `ty_span`: reported, and read as the
    /// `fn sum(…) -> int` it means.
    fn report_c_style_fn(&mut self, name: &Ident, ty_span: Span) {
        let ty_text = ty_span.get_text();
        let before_name = Span {
            end: name.span.start,
            length: name.span.start_offset - ty_span.start_offset,
            ..ty_span.clone()
        };
        let returns = format!(" -> {ty_text}");
        let fix = Fix {
            title: msg!["change to ", code(format!("fn {}(…){returns}", name.name))],
            edits: vec![
                (before_name, "fn ".to_string()),
                (self.previous_span().after(), returns),
            ],
        };
        self.report(
            ty_span,
            msg![
                "som declares functions with ",
                code("fn"),
                ", with the return type after ",
                code("->")
            ],
            msg![
                "write it ",
                code(format!("fn {}(…) -> {ty_text}", name.name))
            ],
            Some(fix),
        );
    }

    /// `-> Ret`, if it's there. A property's type goes after a `:`, as a field's does:
    /// `prop x: i32 = …`.
    fn parse_returns(&mut self, prop: bool, name: &Ident) -> Option<Option<Id<TypeExpr>>> {
        if prop && self.at(TokenKind::Arrow) {
            let span = self.current_span();
            self.advance();
            self.error_at(
                span,
                msg!["a property's type goes after a ", code(":")],
                msg![
                    "as a field's does: ",
                    code(format!("prop {}: …", name.name))
                ],
            );
            return Some(Some(self.parse_type()?));
        }
        if self.void_return() {
            return Some(None);
        }
        if self.eat(TokenKind::Arrow) || prop && self.eat(TokenKind::Colon) {
            return Some(Some(self.parse_type()?));
        }
        Some(None)
    }

    /// A block, or `= value;`, any expression, as in `fn double(x: i32) = x * 2;` and
    /// `prop x = self.values[0];`. `what` it's the body of, for errors.
    pub(super) fn parse_body(&mut self, what: &str) -> Option<Id<Expr>> {
        if !self.eat(TokenKind::Equals) {
            return self.parse_block();
        }
        let value = self.parse_expr()?;
        self.expect(
            TokenKind::Semicolon,
            format!("a semicolon after the {what}'s value"),
        )?;
        Some(value)
    }

    /// A property's parameters, which aren't written: just `&self`. Ones written anyway are
    /// reported and skipped.
    pub(super) fn prop_params(&mut self, name: &Ident) -> Option<Vec<Param>> {
        if self.at(TokenKind::OpenParen) {
            let start = self.current_span();
            self.parse_params(false)?;
            let span = self.span_from(&start);
            self.error_at(
                span,
                "a property has no parameters",
                msg![
                    "its ",
                    code("self"),
                    " is always borrowed: ",
                    code(format!("prop {} -> …", name.name))
                ],
            );
        }
        let ty = self.self_type(Some(false), name.span.clone());
        Some(vec![Param {
            name: Ident {
                name: som_lang::SELF_VALUE.into(),
                span: name.span.clone(),
            },
            ty: Some(ty),
            out: false,
            mutable: false,
        }])
    }

    /// `Self`, or with `reference` saying whether it's mutable, `&Self` or `&mut Self`: the
    /// type of a `self`, `&self` or `&mut self` parameter, written at `span`.
    fn self_type(&mut self, reference: Option<bool>, span: Span) -> Id<TypeExpr> {
        let path = Path {
            segments: vec![Ident {
                name: som_lang::SELF_TYPE.into(),
                span: span.clone(),
            }],
        };
        let named = TypeExpr::Named {
            path,
            args: Vec::new(),
        };
        let to = self.ast.types.alloc(named, span.clone());
        match reference {
            Some(mutable) => {
                let reference = TypeExpr::Reference {
                    mutable,
                    lifetime: None,
                    to,
                };
                self.ast.types.alloc(reference, span)
            }
            None => to,
        }
    }

    /// `<T, N: usize, Other = Self>`, if it's there: a type parameter is a name, a value
    /// parameter a name and its type. Only a struct's or a trait's parameters can have a
    /// default, `with_defaults`.
    pub(super) fn parse_generics(&mut self, with_defaults: bool) -> Option<Vec<Generic>> {
        if !self.eat(TokenKind::LessThan) {
            return Some(Vec::new());
        }
        let mut defaulted = false;
        let params = self.parse_comma_list(TokenKind::GreaterThan, |p| {
            p.parse_generic(with_defaults, &mut defaulted)
        })?;
        self.expect(
            TokenKind::GreaterThan,
            "a closing angle bracket for type parameters",
        )?;
        Some(params)
    }

    fn parse_generic(&mut self, with_defaults: bool, defaulted: &mut bool) -> Option<Generic> {
        let name = self.parse_ident("a generic parameter name")?;
        let (mut ty, mut bounds, mut signature) = (None, Vec::new(), None);
        if self.eat(TokenKind::Colon) {
            // `N: usize` is a value parameter; `T: Hash + Eq` a type that has to implement
            // traits; `F: fn(&T) -> bool` one that can be called so.
            if self.at_primitive_type() {
                ty = Some(self.parse_type()?);
            } else {
                loop {
                    if self.at(TokenKind::Fn) {
                        let at = self.current_span();
                        let written = self.parse_type()?;
                        if signature.replace(written).is_some() {
                            self.error_at(
                                self.span_from(&at),
                                "a parameter is called one way",
                                "a second way to call it",
                            );
                        }
                    } else {
                        bounds.push(self.parse_bound()?);
                    }
                    if !self.eat(TokenKind::Plus) {
                        break;
                    }
                }
            }
        }
        let mut default = None;
        let at = self.current_span();
        if self.eat(TokenKind::Equals) {
            default = Some(self.parse_type()?);
            if !with_defaults {
                self.error_at(
                    at.clone(),
                    "only a struct's or a trait's parameters have defaults",
                    "a default isn't allowed here",
                );
            } else if ty.is_some() {
                self.error_at(
                    at.clone(),
                    "a value parameter has no default",
                    "a default isn't allowed here",
                );
            }
            *defaulted = true;
        } else if *defaulted && with_defaults {
            // `<A = i32, B>`: which one does `<u8>` give?
            self.error_at(
                name.span.clone(),
                "a parameter without a default can't come after one with a default",
                "give it a default, or move it before those that have one",
            );
        }
        Some(Generic {
            name,
            ty,
            bounds,
            signature,
            default,
        })
    }

    /// `Hash`, or a trait given arguments: `Call<&I::Item, bool>`.
    fn parse_bound(&mut self) -> Option<Bound> {
        let start = self.current_span();
        let trait_ = self.parse_path("a trait")?;
        let args = self.parse_type_args()?;
        Some(Bound {
            trait_,
            args,
            span: self.span_from(&start),
        })
    }

    /// `(a: i32, b)`. Extern functions must annotate every parameter, and may end with
    /// `...` to take extra arguments, like C's `printf`; the `bool` says whether they do.
    pub(super) fn parse_params(&mut self, is_extern: bool) -> Option<(Vec<Param>, bool)> {
        self.with_flag(|c| &mut c.params, true, |p| p.parse_params_in(is_extern))
    }

    fn parse_params_in(&mut self, is_extern: bool) -> Option<(Vec<Param>, bool)> {
        self.expect(TokenKind::OpenParen, "a list of function parameters")?;
        let mut params = Vec::new();
        let mut variadic = false;
        while !self.at(TokenKind::CloseParen) && !self.at_eof() {
            if self.at(TokenKind::Ellipsis) {
                variadic = self.parse_ellipsis(is_extern, params.is_empty())?;
                break;
            }
            params.push(match self.type_first_param() {
                true => self.parse_type_first_param()?,
                false => self.parse_param(is_extern)?,
            });
            if !self.list_continues(TokenKind::CloseParen) {
                break;
            }
        }
        self.expect(
            TokenKind::CloseParen,
            "a closing parenthesis after function parameters",
        )?;
        Some((params, variadic))
    }

    /// `...`, at it: whether the function takes extra arguments.
    fn parse_ellipsis(&mut self, is_extern: bool, first: bool) -> Option<bool> {
        let span = self.current_span();
        self.advance();
        let mut variadic = false;
        if !is_extern {
            self.error_at(
                span,
                "only extern functions can take extra arguments",
                "`...` isn't allowed here",
            );
        } else if first {
            self.error_at(
                span,
                msg!["a ", code("..."), " needs a parameter before it"],
                "C requires one",
            );
        } else {
            variadic = true;
        }
        if !self.at(TokenKind::CloseParen) {
            self.error_missing(
                msg![code("..."), " must come last"],
                "a closing parenthesis",
            );
            return None;
        }
        Some(variadic)
    }

    /// At `string name`: another language's order, the type first.
    fn type_first_param(&self) -> bool {
        (self.at(TokenKind::Ident) || primitive(self.peek()).is_some())
            && self.peek_nth(1) == TokenKind::Ident
            && matches!(self.peek_nth(2), TokenKind::Comma | TokenKind::CloseParen)
    }

    /// `string name`: reported, and read as the `name: string` it means.
    fn parse_type_first_param(&mut self) -> Option<Param> {
        let start = self.current_span();
        let ty = self.parse_type()?;
        let ty_text = self.span_from(&start).get_text();
        let name = self.parse_ident("a function parameter name")?;
        let span = self.span_from(&start);
        let fixed = format!("{}: {ty_text}", name.name);
        self.error(
            span.clone(),
            msg!["som writes a parameter's name first, then its type"],
            msg!["write it ", code(&fixed)],
            Some(Fix::change_to(span, fixed)),
        );
        Some(Param {
            name,
            ty: Some(ty),
            out: false,
            mutable: false,
        })
    }

    fn parse_param(&mut self, is_extern: bool) -> Option<Param> {
        // `&self` and `&mut self`: short for `self: &Self` and `self: &mut Self`.
        let reference = (self.at(TokenKind::Ampersand)
            && (self.self_ahead(1) || self.peek_nth(1) == TokenKind::Mut && self.self_ahead(2)))
        .then(|| {
            let start = self.current_span();
            self.advance();
            (self.eat(TokenKind::Mut), start)
        });
        let out = reference.is_none() && self.eat(TokenKind::Out);
        let mutable = reference.is_none() && !out && self.eat(TokenKind::Mut);
        let name = match self.at(TokenKind::SelfValue) {
            true => self.parse_self(),
            false => self.parse_ident("a function parameter name")?,
        };
        let is_self = &*name.name == som_lang::SELF_VALUE;
        let shorthand = reference.is_some() || is_self && !self.at(TokenKind::Colon);
        if shorthand && !self.ctx.impl_block {
            let span = match &reference {
                Some((_, start)) => self.span_from(start),
                None => name.span.clone(),
            };
            self.error_self_outside_method(span);
        }
        let ty = if let Some((mutable, start)) = reference {
            Some(self.self_type(Some(mutable), self.span_from(&start)))
        } else if is_extern {
            self.expect(TokenKind::Colon, "a colon after function parameter name")?;
            Some(self.parse_type()?)
        } else if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else if is_self {
            // `self` and `mut self`: short for `self: Self`.
            Some(self.self_type(None, name.span.clone()))
        } else {
            None
        };
        Some(Param {
            name,
            ty,
            out,
            mutable,
        })
    }

    fn error_self_outside_method(&mut self, span: Span) {
        self.error_at(
            span,
            msg![code("self"), " only exists in methods"],
            msg![
                "a method is written in an ",
                code("impl"),
                " or ",
                code("trait"),
                " block"
            ],
        );
    }
}

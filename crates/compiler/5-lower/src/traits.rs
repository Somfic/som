//! Traits in lowering: a call of a trait's method becomes a call of the method the type's
//! implementation has; and the language's own traits, `for` over an `Iterator` and `==`
//! through `Eq`, become calls of theirs.

use som_ast::{BinOp, Func, Id, Ident, Span, Stmt};
use som_mir::*;
use som_resolve::{DefId, DefKind, LangTrait};
use som_typeck::traits::{builtin_operator, find_impl};

use super::{FnBuilder, LoopTargets, Lowerer, Subst};

impl Lowerer<'_> {
    /// Trait `trait_`'s method called `name`.
    pub(super) fn trait_method(&self, trait_: DefId, name: &str) -> Id<Func> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            unreachable!("lang traits are traits")
        };
        (self.ast.traits[t].methods.iter().copied())
            .find(|&m| &*self.ast.funcs[m].name.name == name)
            .expect("checked: the trait has the method")
    }

    /// For trait method `func` with type arguments `type_args`, `Self` first: the method of
    /// `Self`'s implementation it is, and that method's type arguments. `None` if it's not a
    /// trait's method, or the implementation doesn't write it and gets the trait's body.
    pub(super) fn dispatch(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
    ) -> Option<(Id<Func>, Vec<TypeId>)> {
        let &(trait_, trait_generics) = self.trait_methods.get(&func)?;
        let &this = type_args.first()?;
        let trait_args = type_args.get(1..1 + trait_generics).unwrap_or(&[]);
        let (index, impl_args) =
            find_impl(&self.typed.impls, &self.mir.types, trait_, this, trait_args, &|t| t)?;
        let name = &self.ast.funcs[func].name.name;
        let method = (self.typed.impls[index].methods.iter().copied())
            .find(|&m| self.ast.funcs[m].name.name == *name)?;
        // The implementation's parameters, then the method's own.
        let own = type_args.get(1 + trait_generics..).unwrap_or(&[]);
        Some((
            method,
            impl_args.into_iter().chain(own.iter().copied()).collect(),
        ))
    }
}

impl Lowerer<'_> {
    /// For trait method `func` of a language trait, with `Self` a type that has its operator
    /// built in: a function doing it, `*a + *b` for `add`, the `Ordering` for `compare`.
    pub(super) fn builtin_method(&mut self, func: Id<Func>, type_args: &[TypeId]) -> Option<FuncId> {
        let &(trait_, _) = self.trait_methods.get(&func)?;
        let lang = self.names.lang_of(trait_)?;
        let &this = type_args.first()?;
        if !builtin_operator(self.mir.types.get(this), lang) {
            return None;
        }
        let key = (func, type_args.to_vec());
        if let Some(&id) = self.instances.get(&key) {
            return Some(id);
        }
        let span = self.ast.funcs.span(func).clone();
        let reference = self.mir.types.intern(Ty::Ref {
            mutable: false,
            to: this,
        });
        let bool = self.mir.types.bool;
        let ret = match lang {
            LangTrait::Eq => bool,
            LangTrait::Compare => {
                let ret = self.typed.func(func).ret;
                let ordering = Subst::new(self.names.type_params(func), type_args, &self.typed.impls)
                    .apply(&mut self.mir.types, ret);
                self.make_structs_in(ordering);
                ordering
            }
            _ => this,
        };
        let name: String = format!(
            "{}::{}",
            self.mir.display_ty(this),
            self.ast.funcs[func].name.name
        )
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ':' { c } else { '_' })
        .collect();

        let mut locals = Arena::new();
        let local = |locals: &mut Arena<LocalDecl>, ty, name: Option<&str>| {
            locals.alloc(LocalDecl {
                ty,
                name: name.map(str::to_string),
                mutable: false,
                span: span.clone(),
            })
        };
        local(&mut locals, ret, None);
        let a = local(&mut locals, reference, Some("self"));
        let b = local(&mut locals, reference, Some("other"));
        let value = |l: Local| Operand::Copy(Place::local(l).project(Projection::Deref));
        let stmt = |kind| Statement {
            kind,
            span: span.clone(),
        };
        let end = |kind| Terminator {
            kind,
            span: span.clone(),
        };
        let mut blocks = Arena::new();
        let op = match lang {
            LangTrait::Add => Some(BinOp::Add),
            LangTrait::Subtract => Some(BinOp::Subtract),
            LangTrait::Multiply => Some(BinOp::Multiply),
            LangTrait::Divide => Some(BinOp::Divide),
            LangTrait::Eq => Some(BinOp::Equals),
            _ => None,
        };
        match op {
            Some(op) => {
                let result = Rvalue::Binary {
                    op,
                    lhs: value(a),
                    rhs: value(b),
                };
                blocks.alloc(Block {
                    stmts: vec![stmt(StatementKind::Assign(Place::local(RETURN), result))],
                    terminator: end(TerminatorKind::Return),
                });
            }
            // Less if `<`, else Equal if `==`, else Greater.
            None => {
                let less = local(&mut locals, bool, None);
                let equal = local(&mut locals, bool, None);
                let variant = |index| {
                    stmt(StatementKind::Assign(
                        Place::local(RETURN),
                        Rvalue::Struct {
                            ty: ret,
                            variant: Some(index),
                            fields: Vec::new(),
                        },
                    ))
                };
                let test = |into: Local, op| {
                    vec![
                        stmt(StatementKind::StorageLive(into)),
                        stmt(StatementKind::Assign(
                            Place::local(into),
                            Rvalue::Binary {
                                op,
                                lhs: value(a),
                                rhs: value(b),
                            },
                        )),
                    ]
                };
                let ids: Vec<BlockId> = (0..5).map(|i| Id::new(i)).collect();
                blocks.alloc(Block {
                    stmts: test(less, BinOp::LessThan),
                    terminator: end(TerminatorKind::Branch {
                        condition: Operand::Copy(Place::local(less)),
                        then: ids[1],
                        otherwise: ids[2],
                    }),
                });
                blocks.alloc(Block {
                    stmts: vec![variant(0)],
                    terminator: end(TerminatorKind::Return),
                });
                blocks.alloc(Block {
                    stmts: test(equal, BinOp::Equals),
                    terminator: end(TerminatorKind::Branch {
                        condition: Operand::Copy(Place::local(equal)),
                        then: ids[3],
                        otherwise: ids[4],
                    }),
                });
                blocks.alloc(Block {
                    stmts: vec![variant(1)],
                    terminator: end(TerminatorKind::Return),
                });
                blocks.alloc(Block {
                    stmts: vec![variant(2)],
                    terminator: end(TerminatorKind::Return),
                });
            }
        }
        let id = self.mir.funcs.alloc(Function {
            name,
            params: vec![reference, reference],
            ret,
            span,
            body: Some(Body {
                locals,
                arg_count: 2,
                outs: Vec::new(),
                blocks,
            }),
            link_name: None,
            library: None,
            is_entry: false,
            variadic: false,
        });
        self.instances.insert(key, id);
        Some(id)
    }
}

impl FnBuilder<'_, '_> {
    /// `for name in iterable { body }` over an `Iterator`, or with `through_iterable`, an
    /// `Iterable` whose `iter` makes one: `next` until it gives `none`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn iterator_loop(
        &mut self,
        stmt: Id<Stmt>,
        name: &Ident,
        mutable: bool,
        iterable: Id<som_ast::Expr>,
        body: &[Id<Stmt>],
        span: Span,
        through_iterable: bool,
    ) {
        let names = self.l.names;
        let iterator = names
            .lang(LangTrait::Iterator)
            .expect("checked: there's an Iterator");
        let next = self.l.trait_method(iterator, "next");
        let def = names.local(stmt).expect("every for loop is resolved");
        let value_ty = self.l.typed.local(def);
        let value_ty = self.ty(value_ty);
        let bool = self.l.mir.types.bool;

        self.in_scope(span.clone(), |b| {
            // The iterator: what's looped over, or what its `iter` makes from it.
            let iterable_ty = b.expr_ty(iterable);
            let (iterator_ty, start) = if through_iterable {
                let trait_ = names.lang(LangTrait::Iterable).expect("checked");
                let iter = b.l.trait_method(trait_, "iter");
                // `iter` borrows what's looped over.
                let (viewed, receiver) = match *b.l.mir.types.get(iterable_ty) {
                    Ty::Ref { to, .. } => (to, b.operand(iterable)),
                    _ => {
                        let place = b.place(iterable);
                        let reference = b.l.mir.types.intern(Ty::Ref {
                            mutable: false,
                            to: iterable_ty,
                        });
                        let temp = b.temp(reference, span.clone());
                        let borrow = Rvalue::Ref {
                            mutable: false,
                            place,
                        };
                        b.assign(Place::local(temp), borrow, span.clone());
                        (iterable_ty, Operand::Copy(Place::local(temp)))
                    }
                };
                let func = b.l.instance(iter, vec![viewed]);
                let ty = b.l.mir.funcs[func].ret;
                let call = Rvalue::Call {
                    func,
                    args: vec![receiver],
                };
                (ty, call)
            } else {
                (iterable_ty, Rvalue::Use(b.operand(iterable)))
            };
            let it = b.new_local(iterator_ty, Some("iterator".into()), true, span.clone());
            b.scopes.last_mut().expect("in scope").push(it);
            b.push(StatementKind::StorageLive(it), span.clone());
            b.assign(Place::local(it), start, span.clone());

            let (header, body_block, exit) = (b.new_block(), b.new_block(), b.new_block());
            b.terminate(TerminatorKind::Goto(header), span.clone());

            // Each time round: the next value, if there is one.
            b.current = header;
            let next = b.l.instance(next, vec![iterator_ty]);
            let got_ty = b.l.mir.funcs[next].ret;
            let reference = b.l.mir.types.intern(Ty::Ref {
                mutable: true,
                to: iterator_ty,
            });
            let lent = b.temp(reference, span.clone());
            let borrow = Rvalue::Ref {
                mutable: true,
                place: Place::local(it),
            };
            b.assign(Place::local(lent), borrow, span.clone());
            let got = b.temp(got_ty, span.clone());
            let call = Rvalue::Call {
                func: next,
                args: vec![Operand::Move(Place::local(lent))],
            };
            b.assign(Place::local(got), call, span.clone());
            let more = b.temp(bool, span.clone());
            let holds = Operand::Copy(Place::local(got).project(Projection::Field(0)));
            b.assign(Place::local(more), Rvalue::Use(holds), span.clone());
            b.terminate(
                TerminatorKind::Branch {
                    condition: Operand::Copy(Place::local(more)),
                    then: body_block,
                    otherwise: exit,
                },
                span.clone(),
            );

            b.current = body_block;
            b.loops.push(LoopTargets {
                exit,
                next: header,
                depth: b.scopes.len(),
                stmt: Some(stmt),
            });
            b.in_scope(span.clone(), |b| {
                let local = b.new_local(
                    value_ty,
                    Some(name.name.to_string()),
                    mutable,
                    name.span.clone(),
                );
                b.bindings.insert(def, local);
                b.scopes.last_mut().expect("in scope").push(local);
                b.push(StatementKind::StorageLive(local), span.clone());
                let value = b.read(Place::local(got).project(Projection::Field(1)), value_ty);
                b.assign(Place::local(local), Rvalue::Use(value), span.clone());
                b.stmts(body, span.clone());
            });
            b.loops.pop();
            b.terminate(TerminatorKind::Goto(header), span.clone());
            b.current = exit;
        });
    }

    /// `lhs op rhs` through the method of trait `lang`, with both borrowed: `lhs == rhs` is
    /// `eq(&lhs, &rhs)`, `lhs < rhs` is `compare(&lhs, &rhs)` being `Less`. `None` if the
    /// values are numbers after all, as a `T: Add` can turn out to be, which have it built in.
    pub(super) fn operator_call(
        &mut self,
        lang: LangTrait,
        op: BinOp,
        lhs: Id<som_ast::Expr>,
        rhs: Id<som_ast::Expr>,
        span: Span,
    ) -> Option<Rvalue> {
        let ty = self.expr_ty(lhs);
        if builtin_operator(self.l.mir.types.get(ty), lang) {
            return None;
        }
        let trait_ = self.l.names.lang(lang).expect("checked: the trait is there");
        let method = lang.operator_method().expect("an operator's trait");
        let method = self.l.trait_method(trait_, method);
        let reference = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: ty,
        });
        let mut args = Vec::new();
        for side in [lhs, rhs] {
            let place = self.place(side);
            let temp = self.temp(reference, span.clone());
            let borrow = Rvalue::Ref {
                mutable: false,
                place,
            };
            self.assign(Place::local(temp), borrow, span.clone());
            args.push(Operand::Copy(Place::local(temp)));
        }
        let func = self.l.instance(method, vec![ty]);
        let call = Rvalue::Call { func, args };
        let result_ty = self.l.mir.funcs[func].ret;
        // What's asked of the result: `!=` is not `eq`; `<` is `compare` giving `Less`, the
        // `Ordering` variant at that index.
        let (test, variant) = match op {
            BinOp::NotEquals => (BinOp::Equals, None),
            BinOp::LessThan => (BinOp::Equals, Some(0)),
            BinOp::GreaterThan => (BinOp::Equals, Some(2)),
            BinOp::LessThanOrEqual => (BinOp::NotEquals, Some(2)),
            BinOp::GreaterThanOrEqual => (BinOp::NotEquals, Some(0)),
            _ => return Some(call),
        };
        let result = self.temp(result_ty, span.clone());
        self.assign(Place::local(result), call, span);
        Some(match variant {
            None => Rvalue::Not(Operand::Copy(Place::local(result))),
            Some(index) => {
                let tag = Place::local(result).project(Projection::Field(0));
                let tag_ty = self.place_ty(&tag);
                Rvalue::Binary {
                    op: test,
                    lhs: Operand::Copy(tag),
                    rhs: self.constant(ConstKind::Int(index), tag_ty),
                }
            }
        })
    }
}

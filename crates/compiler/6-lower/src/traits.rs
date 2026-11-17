//! Traits in lowering: a call of a trait's method becomes a call of the method the type's
//! implementation has; and the language's own traits, `for` over an `Iterator` and `==`
//! through `Eq`, become calls of theirs.

use som_ast::{BinOp, Func, Id, Ident, Span, Stmt};
use som_lang::LangTrait;
use som_mir::*;
use som_resolve::{DefId, DefKind};
use som_typeck::Start;
use som_typeck::traits::{builtin_operator, find_impl};

use super::{FnBuilder, Lowerer, Subst};

/// The local holding what a `for` loop iterates with.
const ITERATOR_NAME: &str = "iterator";

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

    pub(super) fn lang_method(&self, lang: LangTrait) -> Id<Func> {
        let trait_ = self
            .names
            .lang(lang)
            .expect("checked: the standard library has it");
        self.trait_method(trait_, lang.method().expect("the language calls it"))
    }

    pub(super) fn lang_property(&self, lang: LangTrait) -> Id<Func> {
        let trait_ = self
            .names
            .lang(lang)
            .expect("checked: the standard library has it");
        self.trait_method(trait_, lang.property().expect("the language reads it"))
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
        let (index, impl_args) = find_impl(
            &self.typed.impls,
            &self.mir.types,
            trait_,
            this,
            trait_args,
            &|t| t,
        )?;
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
    pub(super) fn builtin_method(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
    ) -> Option<FuncId> {
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
                let ordering =
                    Subst::new(self.names.type_params(func), type_args, &self.typed.impls)
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
        .map(|c| {
            if c.is_alphanumeric() || c == ':' {
                c
            } else {
                '_'
            }
        })
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
            reads: Vec::new(),
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
                two_phase: Vec::new(),
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
    /// `for name in iterable { body }` over an `Iterator`, or what makes one, as `start`
    /// says: `next` until it gives `none`. With `copied`, it gives references to values that
    /// copy, and the loop's variable is a copy.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn iterator_loop(
        &mut self,
        stmt: Id<Stmt>,
        name: &Ident,
        mutable: bool,
        iterable: Id<som_ast::Expr>,
        body: &[Id<Stmt>],
        span: Span,
        start: Start,
        copied: bool,
    ) {
        let next = self.l.lang_method(LangTrait::Iterator);
        let (def, value_ty) = self.loop_variable(stmt);
        let bool = self.l.mir.types.bool;

        self.in_scope(span.clone(), |b| {
            // The iterator: what's looped over, or what its `iter` makes from it.
            let iterable_ty = b.expr_ty(iterable);
            // `into_iter` takes what's looped over; `iter` and `iter_mut` the reference to it.
            let maker = match start {
                Start::Itself => None,
                Start::IntoIter => Some(LangTrait::IntoIterator),
                Start::Iter => Some(LangTrait::Iterable),
                Start::IterMut => Some(LangTrait::IterableMut),
            };
            let (iterator_ty, start) = match maker {
                None => (iterable_ty, Rvalue::Use(b.operand(iterable))),
                Some(lang) => {
                    let method = b.l.lang_method(lang);
                    let viewed = match *b.l.mir.types.get(iterable_ty) {
                        Ty::Ref { to, .. } => to,
                        _ => iterable_ty,
                    };
                    let func = b.l.instance(method, vec![viewed]);
                    let ty = b.l.mir.funcs[func].ret;
                    let call = Rvalue::Call {
                        func,
                        args: vec![b.operand(iterable)],
                    };
                    (ty, call)
                }
            };
            let it = b.new_local(iterator_ty, Some(ITERATOR_NAME.into()), true, span.clone());
            b.declare(it, span.clone());
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
            b.in_loop(exit, header, Some(stmt), |b| {
                b.in_scope(span.clone(), |b| {
                    let local = b.let_local(def, name, value_ty, mutable, span.clone());
                    let mut item = Place::local(got).project(Projection::Field(1));
                    if copied {
                        item = item.project(Projection::Deref);
                    }
                    let value = b.read(item, value_ty);
                    b.assign(Place::local(local), Rvalue::Use(value), span.clone());
                    b.stmts(body, span.clone());
                });
            });
            b.terminate(TerminatorKind::Goto(header), span.clone());
            b.current = exit;
        });
    }

    /// `lhs op rhs` through the method of trait `lang`, with both borrowed: `lhs == rhs` is
    /// `eq(&lhs, &rhs)`, `lhs < rhs` is `compare(&lhs, &rhs)` being `Less`. `None` if the
    /// values are numbers after all, as a `T: Add` can turn out to be, which have it built in.
    pub(super) fn operator_call(
        &mut self,
        expr: Id<som_ast::Expr>,
        lang: LangTrait,
        op: BinOp,
        lhs: Id<som_ast::Expr>,
        rhs: Id<som_ast::Expr>,
        span: Span,
    ) -> Option<Rvalue> {
        // The operands' type: the one not converted to it, as `n` is in `n + " points"`.
        let typed = self.typed.clone();
        let converted = |side| typed.conversion(side).is_some();
        let through = |side| typed.is_read_through(side);
        let side = if converted(lhs) { rhs } else { lhs };
        // A `&string` operand is read through: the operands' type is what it refers to.
        let side_ty = self.expr_ty(side);
        let ty = match (through(side), self.l.mir.types.get(side_ty)) {
            (true, &Ty::Ref { to, .. }) => to,
            _ => side_ty,
        };
        if builtin_operator(self.l.mir.types.get(ty), lang) {
            return None;
        }
        let trait_ = self
            .l
            .names
            .lang(lang)
            .expect("checked: the trait is there");
        let method = self
            .l
            .trait_method(trait_, lang.operator_method().expect("an operator's trait"));
        // `Add<Other, Output>`'s arguments, `Self` first; `Eq` and `Compare` have only `Self`.
        let type_args = match self.type_args(expr) {
            args if args.is_empty() => vec![ty],
            args => args,
        };
        // What's on the right: `Other`, which is `Self` unless the implementation says.
        let other = type_args.get(1).copied().unwrap_or(ty);
        let mut args = Vec::new();
        for (side, ty) in [(lhs, ty), (rhs, other)] {
            let reference = self.l.mir.types.intern(Ty::Ref {
                mutable: false,
                to: ty,
            });
            // Converted, `"score: " + n`: made one first, through its `from`, and lent.
            let place = if converted(side) {
                let value = self.operand(side);
                let made = self.temp(ty, span.clone());
                self.assign(Place::local(made), Rvalue::Use(value), span.clone());
                Place::local(made)
            } else if through(side) {
                self.place(side).project(Projection::Deref)
            } else {
                self.place(side)
            };
            let temp = self.temp(reference, span.clone());
            let borrow = Rvalue::Ref {
                mutable: false,
                place,
            };
            self.assign(Place::local(temp), borrow, span.clone());
            args.push(Operand::Copy(Place::local(temp)));
        }
        let func = self.l.instance(method, type_args);
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

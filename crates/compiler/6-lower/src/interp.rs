use som_ast::{BinOp, Expr, Func, Id};
use som_lang::{LangStruct, LangTrait};
use som_mir::*;
use som_resolve::DefKind;

use super::FnBuilder;

impl FnBuilder<'_, '_> {
    /// `"total: {n}"`: one `string`, made at its full length. Each value is borrowed and
    /// measured with `Display::display_len`; the string is made from the sum and the pieces'
    /// lengths; then the pieces are copied in and the values written, in order.
    pub(super) fn interpolated(&mut self, expr: Id<Expr>, parts: &[som_ast::Part]) -> Rvalue {
        let ast = self.l.ast;
        let names = self.l.names;
        let span = self.span(expr);
        let usize = self.l.mir.types.intern(Ty::Int(IntTy::USize));
        let string_ty = self.expr_ty(expr);
        let display_len = self.l.lang_property(LangTrait::Display);
        let text = (names.lang_struct(LangStruct::String))
            .expect("checked: the standard library has `string`");
        let own = |name: &str| -> Id<Func> {
            let def = names
                .method(text, name)
                .expect("the standard library's `string` has it");
            match names.def(def).kind {
                DefKind::Func(func) => func,
                _ => unreachable!("methods are functions"),
            }
        };
        let (made, write_piece, write_shown) =
            (own("interpolated"), own("write_piece"), own("write_shown"));

        // Each value, borrowed, with the type it's shown as and its length.
        let mut shown = Vec::new();
        // Temporaries holding values made here, `xs.get(1)`: they end once they're written,
        // and the borrows they hold with them.
        let first_new = self.locals.slots();
        let mut fresh = Vec::new();
        let mut written: i128 = 0;
        for &part in parts {
            match part {
                som_ast::Part::Text(piece) => {
                    if let Expr::Str(piece) = &ast.exprs[piece] {
                        written += piece.len() as i128;
                    }
                }
                som_ast::Part::Value(value) => {
                    let mut place = self.place(value);
                    if place.local.id >= first_new && self.locals[place.local].name.is_none() {
                        fresh.push(place.local);
                    }
                    let mut ty = self.expr_ty(value);
                    // Through references, to what they refer to.
                    while let Ty::Ref { to, .. } = *self.l.mir.types.get(ty) {
                        place = place.project(Projection::Deref);
                        ty = to;
                    }
                    let lent_ty = self.l.mir.types.intern(Ty::Ref {
                        mutable: false,
                        to: ty,
                    });
                    let lent = self.temp(lent_ty, span.clone());
                    self.assign(
                        Place::local(lent),
                        Rvalue::Ref {
                            mutable: false,
                            place,
                        },
                        span.clone(),
                    );
                    let measure = self.l.instance(display_len, vec![ty]);
                    let len = self.temp(usize, span.clone());
                    let call = Rvalue::Call {
                        func: measure,
                        args: vec![Operand::Copy(Place::local(lent))],
                    };
                    self.assign(Place::local(len), call, span.clone());
                    shown.push((lent, ty, len));
                }
            }
        }

        // How long it is: the pieces, and each value.
        let total = self.temp(usize, span.clone());
        let pieces = self.constant(ConstKind::Int(written), usize);
        self.assign(Place::local(total), Rvalue::Use(pieces), span.clone());
        for &(_, _, len) in &shown {
            let sum = Rvalue::Binary {
                op: BinOp::Add,
                lhs: Operand::Copy(Place::local(total)),
                rhs: Operand::Copy(Place::local(len)),
            };
            self.assign(Place::local(total), sum, span.clone());
        }
        let out = self.temp(string_ty, span.clone());
        let make = Rvalue::Call {
            func: self.l.instance(made, Vec::new()),
            args: vec![Operand::Copy(Place::local(total))],
        };
        self.assign(Place::local(out), make, span.clone());

        // Filled in, in order: `at` is where the next part goes.
        let out_ty = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: string_ty,
        });
        let lent_out = self.temp(out_ty, span.clone());
        let borrow = Rvalue::Ref {
            mutable: false,
            place: Place::local(out),
        };
        self.assign(Place::local(lent_out), borrow, span.clone());
        let at = self.temp(usize, span.clone());
        let zero = self.constant(ConstKind::Int(0), usize);
        self.assign(Place::local(at), Rvalue::Use(zero), span.clone());
        let mut values = shown.into_iter();
        for &part in parts {
            let call = match part {
                som_ast::Part::Text(piece) => Rvalue::Call {
                    func: self.l.instance(write_piece, Vec::new()),
                    args: vec![
                        Operand::Copy(Place::local(lent_out)),
                        Operand::Copy(Place::local(at)),
                        self.operand(piece),
                    ],
                },
                som_ast::Part::Value(_) => {
                    let (lent, ty, len) = values.next().expect("one for each value");
                    Rvalue::Call {
                        func: self.l.instance(write_shown, vec![ty]),
                        args: vec![
                            Operand::Copy(Place::local(lent_out)),
                            Operand::Copy(Place::local(at)),
                            Operand::Move(Place::local(lent)),
                            Operand::Copy(Place::local(len)),
                        ],
                    }
                }
            };
            self.assign(Place::local(at), call, span.clone());
        }
        self.push(StatementKind::StorageDead(lent_out), span.clone());
        // A value that's dropped is the statement's to drop; one that copies just ends.
        for temp in fresh {
            if self.l.drop_glue(self.locals[temp].ty).is_none() {
                self.push(StatementKind::StorageDead(temp), span.clone());
            }
        }
        Rvalue::Use(Operand::Move(Place::local(out)))
    }
}

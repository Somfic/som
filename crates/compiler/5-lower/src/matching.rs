//! `match`: each arm's pattern becomes a chain of tests on the matched place, in order. The
//! first arm whose tests all pass binds its names and runs; a failed test goes on to the next
//! arm. The type checker made sure some arm always matches.

use som_ast::{Arm, Expr, Id, Pattern, PatternFields};
use som_mir::*;
use som_resolve::{DefId, DefKind};

use super::FnBuilder;

/// A name an arm binds, once its tests pass: to a copy (or move) of `place`, or with `by_ref`,
/// a reference to it.
struct Binding {
    def: DefId,
    place: Place,
    by_ref: Option<bool>,
}

impl FnBuilder<'_, '_> {
    pub(super) fn lower_match(
        &mut self,
        expr: Id<Expr>,
        scrutinee: Id<Expr>,
        arms: &[Arm],
    ) -> Operand {
        let ty = self.expr_ty(expr);
        let span = self.span(expr);
        let unit = self.l.mir.types.unit;
        let result = (ty != unit).then(|| self.temp(ty, span.clone()));

        // What's matched: through a reference, what it points to.
        let scrutinee_ty = self.expr_ty(scrutinee);
        let mut place = self.place(scrutinee);
        let mut matched_ty = scrutinee_ty;
        if let Ty::Ref { to, .. } = *self.l.mir.types.get(scrutinee_ty) {
            place = place.project(Projection::Deref);
            matched_ty = to;
        }

        let join = self.new_block();
        for arm in arms {
            let next = self.new_block();
            let mut bindings = Vec::new();
            self.test_pattern(
                arm.pattern,
                place.clone(),
                matched_ty,
                next,
                &mut bindings,
            );
            let diverges = self.l.typed.diverges(arm.body);
            self.in_scope(span.clone(), |b| {
                for binding in bindings {
                    b.bind(binding);
                }
                let value = b.operand(arm.body);
                if let Some(result) = result
                    && !diverges
                {
                    b.assign(Place::local(result), Rvalue::Use(value), span.clone());
                }
            });
            // An arm that never finishes (`{ return 0; }`) doesn't get to the end.
            let after = if diverges {
                TerminatorKind::Unreachable
            } else {
                TerminatorKind::Goto(join)
            };
            self.terminate(after, span.clone());
            self.current = next;
        }
        // Past the last arm: some arm always matches.
        self.terminate(TerminatorKind::Unreachable, span.clone());

        self.current = join;
        match result {
            Some(result) => self.read(Place::local(result), ty),
            None => self.constant(ConstKind::Unit, unit),
        }
    }

    /// Tests that the value at `place`, of type `ty`, matches pattern `id`, going to `fail` if
    /// it doesn't; and adds the names it binds to `bindings`.
    fn test_pattern(
        &mut self,
        id: Id<Pattern>,
        mut place: Place,
        mut ty: TypeId,
        fail: BlockId,
        bindings: &mut Vec<Binding>,
    ) {
        let ast = self.l.ast;
        let span = ast.patterns.span(id).clone();
        let info = self.l.typed.pattern(id).cloned().unwrap_or_default();
        // A pattern for an optional's value: it has to hold one.
        if info.unwraps {
            let holds = Operand::Copy(place.clone().project(Projection::Field(0)));
            self.test(holds, fail, span.clone());
            let Ty::Optional(inner) = *self.l.mir.types.get(ty) else {
                unreachable!("checked: only optionals are unwrapped")
            };
            place = place.project(Projection::Field(1));
            ty = inner;
        }

        if let Some(variant) = info.variant {
            self.test_variant(&place, variant, span.clone(), fail);
            let Pattern::Variant { fields, .. } = &ast.patterns[id] else {
                // A bare name that's a variant: it holds nothing.
                return;
            };
            let subs: Vec<Id<Pattern>> = match fields {
                PatternFields::Unit => Vec::new(),
                PatternFields::Tuple(pats) => pats.clone(),
                PatternFields::Named { fields, .. } => fields.iter().map(|&(_, p)| p).collect(),
            };
            let first = self.variant_fields(ty, variant).start;
            for (sub, &field) in subs.into_iter().zip(&info.fields) {
                let field_place = place.clone().project(Projection::Field(first + field));
                let field_ty = self.place_ty(&field_place);
                self.test_pattern(sub, field_place, field_ty, fail, bindings);
            }
            return;
        }

        let bool = self.l.mir.types.bool;
        match &ast.patterns[id] {
            Pattern::Wildcard | Pattern::Variant { .. } => {}
            Pattern::Binding { .. } => {
                let def = (self.l.names.pattern(id))
                    .filter(|&d| matches!(self.l.names.def(d).kind, DefKind::Binding(_)))
                    .expect("checked: a binding that isn't a variant binds a name");
                bindings.push(Binding {
                    def,
                    place,
                    by_ref: info.by_ref,
                });
            }
            Pattern::Int(n) => {
                let value = self.constant(ConstKind::Int(*n), ty);
                let equal = self.compute(
                    Rvalue::Binary {
                        op: BinOp::Equals,
                        lhs: Operand::Copy(place),
                        rhs: value,
                    },
                    bool,
                    span.clone(),
                );
                self.test(equal, fail, span);
            }
            Pattern::Bool(b) => {
                let value = Operand::Copy(place);
                let wanted = match b {
                    true => value,
                    false => self.compute(Rvalue::Not(value), bool, span.clone()),
                };
                self.test(wanted, fail, span);
            }
            Pattern::None => {
                let holds = Operand::Copy(place.project(Projection::Field(0)));
                let empty = self.compute(Rvalue::Not(holds), bool, span.clone());
                self.test(empty, fail, span);
            }
        }
    }

    /// Tests that the enum at `place` is variant `variant`.
    fn test_variant(
        &mut self,
        place: &Place,
        variant: usize,
        span: som_ast::Span,
        fail: BlockId,
    ) {
        let bool = self.l.mir.types.bool;
        let tag_place = place.clone().project(Projection::Field(0));
        let tag_ty = self.place_ty(&tag_place);
        let wanted = self.constant(ConstKind::Int(variant as i64), tag_ty);
        let is_variant = self.compute(
            Rvalue::Binary {
                op: BinOp::Equals,
                lhs: Operand::Copy(tag_place),
                rhs: wanted,
            },
            bool,
            span.clone(),
        );
        self.test(is_variant, fail, span);
    }

    /// Where variant `variant`'s fields are among enum `ty`'s.
    fn variant_fields(&mut self, ty: TypeId, variant: usize) -> std::ops::Range<usize> {
        let Ty::Struct(def, args) = self.l.mir.types.get(ty).clone() else {
            unreachable!("checked: variants are of enums")
        };
        self.l.mir.struct_def(def, &args).variant_fields(variant)
    }

    /// Goes on if `condition` holds, and to `fail` if not.
    fn test(&mut self, condition: Operand, fail: BlockId, span: som_ast::Span) {
        let pass = self.new_block();
        self.terminate(
            TerminatorKind::Branch {
                condition,
                then: pass,
                otherwise: fail,
            },
            span,
        );
        self.current = pass;
    }

    /// Stores `rvalue`, of type `ty`, in a temporary, and reads it.
    fn compute(&mut self, rvalue: Rvalue, ty: TypeId, span: som_ast::Span) -> Operand {
        let temp = self.temp(ty, span.clone());
        self.assign(Place::local(temp), rvalue, span);
        Operand::Copy(Place::local(temp))
    }

    /// Declares the local for a name an arm binds, in the arm's scope, and fills it in.
    fn bind(&mut self, binding: Binding) {
        let Binding { def, place, by_ref } = binding;
        let ty = self.l.typed.local(def);
        let ty = self.ty(ty);
        let def_info = self.l.names.def(def);
        let DefKind::Binding(pattern) = def_info.kind else {
            unreachable!("bindings are made for binding patterns")
        };
        let mutable = matches!(
            self.l.ast.patterns[pattern],
            Pattern::Binding { mutable: true, .. }
        );
        let span = def_info.span.clone();
        let local = self.new_local(ty, Some(def_info.name.to_string()), mutable, span.clone());
        self.bindings.insert(def, local);
        self.scopes
            .last_mut()
            .expect("arms are bound in a scope")
            .push(local);
        self.push(StatementKind::StorageLive(local), span.clone());
        let value = match by_ref {
            Some(mutable) => Rvalue::Ref { mutable, place },
            None => {
                let place_ty = self.place_ty(&place);
                Rvalue::Use(self.read(place, place_ty))
            }
        };
        self.assign(Place::local(local), value, span);
    }
}

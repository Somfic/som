//! `dyn Trait`: in MIR, a struct holding where the value is, on the heap, a number saying
//! which type it is, and a function for dropping it and for each of the trait's methods.
//! Made from a value of a type implementing the trait, where a `dyn` is expected; a call of
//! one of its methods is a call through the function it holds.

use som_ast::{Expr, Func, Id, Span};
use som_mir::*;
use som_resolve::{DefId, DefKind};

use super::instances::{Subst, internal_function};
use super::{FnBuilder, Lowerer};

/// Its fields: where the value is, which type it is, how it's dropped, then each method.
const VALUE: usize = 0;
const TYPE: usize = 1;
const DROP: usize = 2;
const METHODS: usize = 3;

impl Lowerer<'_> {
    /// For the type a `dyn` is in MIR: the trait, and the trait's arguments.
    pub(super) fn dyn_of(&self, ty: TypeId) -> Option<(DefId, Vec<TypeId>)> {
        match self.mir.types.get(ty) {
            Ty::Struct(def, args) if matches!(self.names.def(*def).kind, DefKind::Trait(_)) => {
                Some((*def, args.clone()))
            }
            _ => None,
        }
    }

    /// The trait's methods a `dyn` holds: those its implementations write. The others, with
    /// a body of their own, are called with `Self` the `dyn`.
    fn dyn_methods(&self, trait_: DefId) -> Vec<Id<Func>> {
        let DefKind::Trait(t) = self.names.def(trait_).kind else {
            unreachable!("a dyn's trait")
        };
        (self.ast.traits[t].methods.iter().copied())
            .filter(|&m| self.ast.funcs[m].body.is_none())
            .collect()
    }

    /// `method`'s parameters and what it gives, with `Self` and the trait's parameters
    /// `type_args`.
    fn method_sig(&mut self, method: Id<Func>, type_args: &[TypeId]) -> (Vec<TypeId>, TypeId) {
        let subst = Subst::new(self.names.type_params(method), type_args, &self.typed.impls);
        let sig = self.typed.func(method);
        let params = (sig.params.iter())
            .map(|&p| subst.apply(&mut self.mir.types, p))
            .collect();
        let ret = subst.apply(&mut self.mir.types, sig.ret);
        (params, ret)
    }

    /// The struct `dyn trait_<args>` is.
    pub(super) fn dyn_struct(&mut self, trait_: DefId, args: Vec<TypeId>) {
        let pointer = self.mir.types.pointer;
        let unit = self.mir.types.unit;
        let number = self.mir.types.intern(Ty::Int(IntTy::U64));
        let drop = self.mir.types.intern(Ty::Fn {
            params: vec![pointer],
            ret: unit,
        });
        let mut fields = vec![
            ("value".to_string(), pointer),
            ("type".to_string(), number),
            ("drop".to_string(), drop),
        ];
        // Registered before its methods' types are worked out, which can name it.
        let ty = self.mir.types.intern(Ty::Struct(trait_, args.clone()));
        let this: Vec<TypeId> = std::iter::once(ty).chain(args.iter().copied()).collect();
        for method in self.dyn_methods(trait_) {
            let (params, ret) = self.method_sig(method, &this);
            let params = std::iter::once(pointer).chain(params.into_iter().skip(1)).collect();
            let ty = self.mir.types.intern(Ty::Fn { params, ret });
            fields.push((self.ast.funcs[method].name.name.to_string(), ty));
        }
        let name = format!("dyn {}", self.names.def(trait_).name);
        self.mir.add_struct(StructDef {
            def: trait_,
            args,
            name,
            fields,
            variants: None,
            copy: false,
            handle: false,
        });
        self.mir.drop_types.insert(ty);
    }

    /// For trait method `func` with `Self` a `dyn`, `type_args[0]`: a function calling the
    /// one the `dyn` holds for it. `None` if `Self` isn't one, or the method has a body of
    /// its own, which is lowered as any other.
    pub(super) fn dyn_call(&mut self, func: Id<Func>, type_args: &[TypeId]) -> Option<FuncId> {
        let &this = type_args.first()?;
        let (trait_, _) = self.dyn_of(this)?;
        let index = self.dyn_methods(trait_).iter().position(|&m| m == func)?;
        let key = (func, type_args.to_vec());
        if let Some(&id) = self.dyn_calls.get(&key) {
            return Some(id);
        }
        self.make_structs_in(this);
        let (params, ret) = self.method_sig(func, type_args);
        let span = self.ast.funcs.span(func).clone();
        let mut b = Shim::new(span.clone(), ret, &params);
        // `self` is a reference to the `dyn`.
        let held = Place::local(Id::new(1)).project(Projection::Deref);
        let callee = Operand::Copy(held.clone().project(Projection::Field(METHODS + index)));
        let value = Operand::Copy(held.project(Projection::Field(VALUE)));
        let args = std::iter::once(value)
            .chain((2..=params.len()).map(|i| Operand::Move(Place::local(Id::new(i)))))
            .collect();
        b.assign(Place::local(RETURN), Rvalue::CallIndirect { callee, args });
        let name = format!(
            "dyn {}::{}",
            self.names.def(trait_).name,
            self.ast.funcs[func].name.name
        );
        let id = self.alloc_func(b.finish(name, params, ret));
        self.dyn_calls.insert(key, id);
        Some(id)
    }

    /// For a value of type `ty` made a `dyn` of type `dyn_ty`: the functions it holds, after
    /// its value and type. Each takes where the value is, as a `*`.
    fn dyn_functions(&mut self, ty: TypeId, dyn_ty: TypeId) -> Vec<Operand> {
        let (trait_, args) = self.dyn_of(dyn_ty).expect("a dyn");
        let mut functions = vec![self.dyn_drop(ty)];
        let this: Vec<TypeId> = std::iter::once(ty).chain(args.iter().copied()).collect();
        for method in self.dyn_methods(trait_) {
            functions.push(self.dyn_method(method, &this, dyn_ty));
        }
        let fields = self.mir.struct_def(trait_, &args).fields.clone();
        (functions.into_iter().zip(&fields[DROP..]))
            .map(|(f, &(_, ty))| Operand::Const(Const {
                kind: ConstKind::Func(f),
                ty,
            }))
            .collect()
    }

    /// Drops the `ty` at a `*`, and gives back its memory.
    fn dyn_drop(&mut self, ty: TypeId) -> FuncId {
        if let Some(&id) = self.dyn_drops.get(&ty) {
            return id;
        }
        let pointer = self.mir.types.pointer;
        let unit = self.mir.types.unit;
        let span = self.span_of(ty);
        let mut b = Shim::new(span, unit, &[pointer]);
        let at = b.typed_pointer(ty, &mut self.mir.types);
        let usize = self.mir.types.usize;
        let value = b.local(ty);
        b.assign(
            Place::local(value),
            Rvalue::Intrinsic {
                intrinsic: Intrinsic::Take,
                args: vec![Operand::Copy(Place::local(at)), int(0, usize)],
                ty,
            },
        );
        if let Some(glue) = self.drop_glue(ty) {
            b.drop(Place::local(value), glue);
        }
        let done = b.local(unit);
        b.assign(
            Place::local(done),
            Rvalue::Intrinsic {
                intrinsic: Intrinsic::Free,
                args: vec![Operand::Copy(Place::local(at))],
                ty: unit,
            },
        );
        b.assign(Place::local(RETURN), Rvalue::Use(unit_value(unit)));
        let name = format!("dyn drop<{}>", self.mir.display_ty(ty));
        let id = self.alloc_func(b.finish(name, vec![pointer], unit));
        self.dyn_drops.insert(ty, id);
        id
    }

    /// Calls `method` of `this[0]`, given where the value is as a `*`.
    fn dyn_method(&mut self, method: Id<Func>, this: &[TypeId], dyn_ty: TypeId) -> FuncId {
        let key = (method, this.to_vec());
        if let Some(&id) = self.dyn_methods_of.get(&key) {
            return id;
        }
        let ty = this[0];
        let pointer = self.mir.types.pointer;
        let (params, ret) = self.method_sig(method, this);
        let receiver = params[0];
        let shim_params: Vec<TypeId> = std::iter::once(pointer).chain(params[1..].iter().copied()).collect();
        let span = self.ast.funcs.span(method).clone();
        let mut b = Shim::new(span, ret, &shim_params);
        let at = b.typed_pointer(ty, &mut self.mir.types);
        let usize = self.mir.types.usize;
        let lent = b.local(receiver);
        b.assign(
            Place::local(lent),
            Rvalue::Intrinsic {
                intrinsic: Intrinsic::Offset,
                args: vec![Operand::Copy(Place::local(at)), int(0, usize)],
                ty: receiver,
            },
        );
        let func = self.instance(method, this.to_vec());
        let args = std::iter::once(Operand::Move(Place::local(lent)))
            .chain((2..=shim_params.len()).map(|i| Operand::Move(Place::local(Id::new(i)))))
            .collect();
        b.assign(Place::local(RETURN), Rvalue::Call { func, args });
        let name = format!(
            "{} as {}::{}",
            self.mir.display_ty(ty),
            self.mir.display_ty(dyn_ty),
            self.ast.funcs[method].name.name
        );
        let id = self.alloc_func(b.finish(name, shim_params, ret));
        self.dyn_methods_of.insert(key, id);
        id
    }

    /// What a `dyn`'s drop glue does itself: calls the function it holds for it.
    pub(super) fn dyn_glue(&mut self, ty: TypeId) -> FuncId {
        let unit = self.mir.types.unit;
        let reference = self.mir.types.intern(Ty::Ref {
            mutable: true,
            to: ty,
        });
        let span = self.span_of(ty);
        let mut b = Shim::new(span, unit, &[reference]);
        let held = Place::local(Id::new(1)).project(Projection::Deref);
        let callee = Operand::Copy(held.clone().project(Projection::Field(DROP)));
        let value = Operand::Copy(held.project(Projection::Field(VALUE)));
        b.assign(
            Place::local(RETURN),
            Rvalue::CallIndirect {
                callee,
                args: vec![value],
            },
        );
        let name = format!("drop<{}>", self.mir.display_ty(ty));
        self.alloc_func(b.finish(name, vec![reference], unit))
    }

    /// A number for each type a `dyn` is made of, saying which it is.
    fn type_number(&mut self, ty: TypeId) -> i128 {
        let next = self.type_numbers.len() as i128 + 1;
        *self.type_numbers.entry(ty).or_insert(next)
    }

    fn span_of(&self, ty: TypeId) -> Span {
        match self.mir.types.get(ty) {
            Ty::Struct(def, _) => self.names.def(*def).span.clone(),
            _ => self.at.clone().unwrap_or_else(|| self.ast.funcs.span(Id::new(0)).clone()),
        }
    }
}

impl FnBuilder<'_, '_> {
    /// `value` of type `ty`, made a `dyn_ty`: moved to the heap, with its type's functions.
    pub(super) fn make_dyn(&mut self, value: Operand, ty: TypeId, dyn_ty: TypeId, span: Span) -> Operand {
        self.l.make_structs_in(dyn_ty);
        let at_ty = self.l.mir.types.intern(Ty::Ptr(ty));
        let usize = self.l.mir.types.usize;
        let unit = self.l.mir.types.unit;
        let at = self.temp(at_ty, span.clone());
        let null = self.constant(ConstKind::Zeroed, at_ty);
        self.assign(Place::local(at), Rvalue::Use(null), span.clone());
        let room = Rvalue::Intrinsic {
            intrinsic: Intrinsic::Resize,
            args: vec![Operand::Copy(Place::local(at)), int(1, usize)],
            ty: at_ty,
        };
        self.assign(Place::local(at), room, span.clone());
        let put = Rvalue::Intrinsic {
            intrinsic: Intrinsic::Put,
            args: vec![Operand::Copy(Place::local(at)), int(0, usize), value],
            ty: unit,
        };
        self.temp_read(unit, put, span.clone());
        let pointer = self.l.mir.types.pointer;
        let untyped = Rvalue::Cast {
            operand: Operand::Copy(Place::local(at)),
            ty: pointer,
        };
        let untyped = self.temp_read(pointer, untyped, span.clone());
        let number = self.l.mir.types.intern(Ty::Int(IntTy::U64));
        let which = int(self.l.type_number(ty), number);
        let fields = [untyped, which]
            .into_iter()
            .chain(self.l.dyn_functions(ty, dyn_ty))
            .collect();
        let made = Rvalue::Struct {
            ty: dyn_ty,
            variant: None,
            fields,
        };
        self.temp_read(dyn_ty, made, span)
    }

    /// `expr`, which the checker said becomes a `dyn` where it's used.
    pub(super) fn made_dyn(&mut self, expr: Id<Expr>, value: Operand) -> Operand {
        let ty = self.expr_ty(expr);
        let dyn_ty = self.typed.made_dyn(expr).expect("made a dyn");
        let dyn_ty = self.ty(dyn_ty);
        self.make_dyn(value, ty, dyn_ty, self.span(expr))
    }

    /// Whether the `dyn` at `place`, of type `dyn_ty`, holds a `ty`, branching to `otherwise`
    /// if not; and a reference to it, if it does.
    pub(super) fn dyn_holds(
        &mut self,
        place: Place,
        ty: TypeId,
        otherwise: BlockId,
        span: Span,
    ) -> Place {
        let number = self.l.mir.types.intern(Ty::Int(IntTy::U64));
        let which = int(self.l.type_number(ty), number);
        let bool = self.l.mir.types.bool;
        let is = Rvalue::Binary {
            op: BinOp::Equals,
            lhs: Operand::Copy(place.clone().project(Projection::Field(TYPE))),
            rhs: which,
        };
        let is = self.temp_copy(bool, is, span.clone());
        self.test(is, otherwise, span.clone());
        let at_ty = self.l.mir.types.intern(Ty::Ptr(ty));
        let at = Rvalue::Cast {
            operand: Operand::Copy(place.project(Projection::Field(VALUE))),
            ty: at_ty,
        };
        let at = self.temp_read(at_ty, at, span.clone());
        let usize = self.l.mir.types.usize;
        let to = self.l.mir.types.intern(Ty::Ref {
            mutable: false,
            to: ty,
        });
        let lent = Rvalue::Intrinsic {
            intrinsic: Intrinsic::Offset,
            args: vec![at, int(0, usize)],
            ty: to,
        };
        let reference = self.temp(to, span.clone());
        self.assign(Place::local(reference), lent, span);
        Place::local(reference).project(Projection::Deref)
    }
}

fn int(n: i128, ty: TypeId) -> Operand {
    Operand::Const(Const {
        kind: ConstKind::Int(n),
        ty,
    })
}

fn unit_value(unit: TypeId) -> Operand {
    Operand::Const(Const {
        kind: ConstKind::Unit,
        ty: unit,
    })
}

/// A function of the compiler's own being built, one block long: its parameters, then
/// what it does.
struct Shim {
    locals: Arena<LocalDecl>,
    block: Block,
    span: Span,
}

impl Shim {
    fn new(span: Span, ret: TypeId, params: &[TypeId]) -> Shim {
        let mut b = Shim {
            locals: Arena::new(),
            block: Block {
                stmts: Vec::new(),
                terminator: Terminator {
                    kind: TerminatorKind::Return,
                    span: span.clone(),
                },
            },
            span,
        };
        b.local(ret);
        for &param in params {
            b.local(param);
        }
        b
    }

    fn local(&mut self, ty: TypeId) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            name: None,
            mutable: true,
            span: self.span.clone(),
        })
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue) {
        self.block.stmts.push(Statement {
            kind: StatementKind::Assign(place, rvalue),
            span: self.span.clone(),
            reads: Vec::new(),
        });
    }

    fn drop(&mut self, place: Place, glue: FuncId) {
        self.block.stmts.push(Statement {
            kind: StatementKind::Drop {
                place,
                glue,
                flag: None,
            },
            span: self.span.clone(),
            reads: Vec::new(),
        });
    }

    /// The first parameter, a `*`, as a `*ty`.
    fn typed_pointer(&mut self, ty: TypeId, types: &mut TypeTable) -> Local {
        let at_ty = types.intern(Ty::Ptr(ty));
        let at = self.local(at_ty);
        self.assign(
            Place::local(at),
            Rvalue::Cast {
                operand: Operand::Copy(Place::local(Id::new(1))),
                ty: at_ty,
            },
        );
        at
    }

    fn finish(self, name: String, params: Vec<TypeId>, ret: TypeId) -> Function {
        let mut blocks = Arena::new();
        blocks.alloc(self.block);
        let mut function = internal_function(name, params.clone(), ret, self.span);
        function.body = Some(Body {
            locals: self.locals,
            arg_count: params.len(),
            outs: Vec::new(),
            two_phase: Vec::new(),
            blocks,
        });
        function
    }
}

use som_ast::{Expr, Func, Id, Span};
use som_lang::LangTrait;
use som_resolve::visibility::{self};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};
use crate::unify::{Unifier, VarKind};
use crate::{ArrayMethod, PointerMethod};

/// How much a method call's receiver changes to be the method's `self`: of the methods
/// sharing a name, the one changing it least is chosen.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ReceiverCost {
    AsItIs,
    /// Borrowed as a `&`, or copied out of a reference.
    Changed,
    BorrowedMut,
}

impl Checker<'_> {
    pub(super) fn infer_method_call(
        &mut self,
        id: Id<Expr>,
        receiver: Id<Expr>,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
    ) -> TypeId {
        let error = self.u.table.error;
        let expected = self.hints.expected_here.take();
        let receiver_ty = self.infer(receiver);
        // `node.total()` of a `#Node`: what it shares.
        let receiver_ty = self.through_shared(receiver, receiver_ty, &method.name);
        // `"abc".len()`: text in quotes is a `string`, with its methods.
        self.text_as_string(receiver_ty);
        // A trait's method the type has, `x.compare(&y)`, before a pointer's own; an
        // array's or slice's own first, `get` giving a copy.
        let builtin = ArrayMethod::from_name(&method.name).is_some() && {
            let mut ty = self.u.shallow(receiver_ty);
            if let Ty::Ref { to, .. } = *self.u.get(ty) {
                ty = self.u.shallow(to);
            }
            matches!(self.u.get(ty), Ty::Array { .. } | Ty::Slice(_) | Ty::Str)
        };
        if !builtin
            && let Some(ty) = self.trait_method_call(id, receiver, receiver_ty, method, args)
        {
            return ty;
        }
        // A built-in type's own method, from `impl i32 { … }`.
        let primitive = self
            .primitive_owner(receiver_ty)
            .filter(|&owner| self.names.method(owner, &method.name).is_some());
        if primitive.is_none() {
            if let Some(ty) = self.array_method(id, receiver, receiver_ty, method, args, false) {
                return ty;
            }
            if let Some(ty) = self.pointer_method(id, receiver_ty, method, args) {
                return ty;
            }
        }
        // Text written in quotes is a `string` here.
        if primitive.is_some() && self.u.var_kind(receiver_ty) == Some(VarKind::Text) {
            let _ = self.u.unify(receiver_ty, self.u.table.text);
        }
        // Any type can have methods now: one that isn't a struct just lacks this one.
        let viewed = {
            let shallow = self.u.shallow(receiver_ty);
            match self.u.get(shallow) {
                Ty::Ref { to, .. } => self.u.shallow(*to),
                _ => shallow,
            }
        };
        let lacks = !matches!(self.u.get(viewed), Ty::Struct(..) | Ty::Error)
            && !matches!(self.u.var_kind(viewed), Some(VarKind::Any));
        // `find(2).get()`: an optional may hold nothing, which is what to say.
        if primitive.is_none() && matches!(self.u.get(viewed), Ty::Optional(_)) {
            let span = self.span(receiver);
            self.errors.push(TypeError::MemberOfOptional {
                object: span.get_text().to_string(),
                ty: self.describe(receiver_ty),
                variable: matches!(self.ast.exprs[receiver], Expr::Var(_)),
                span,
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return error;
        }
        if primitive.is_none() && lacks {
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(receiver_ty),
                method: method.name.to_string(),
                span: method.span.clone(),
                methods: Vec::new(),
                props: Vec::new(),
                fields: Vec::new(),
                bare_call: self.bare_call(id, method, args),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return error;
        }
        let owner = match primitive {
            Some(owner) => owner,
            None => match self.struct_of(receiver_ty, receiver) {
                Some((owner, _)) => owner,
                None => {
                    args.iter().for_each(|&a| {
                        self.infer(a);
                    });
                    return error;
                }
            },
        };
        let Some(def) = self.names.method(owner, &method.name) else {
            let ty = self.describe(receiver_ty);
            if !self.names.props(owner, &method.name).is_empty() {
                let span = self.span(id);
                self.errors.push(TypeError::MethodIsProp {
                    ty,
                    name: method.name.to_string(),
                    span: method.span.clone(),
                    parens: args
                        .is_empty()
                        .then(|| method.span.after().merge(&span.after())),
                });
                args.iter().for_each(|&a| {
                    self.infer(a);
                });
                return error;
            }
            let (fields, props, methods) = self.members(owner, receiver_ty, &method.span);
            self.errors.push(TypeError::UnknownMethod {
                ty,
                method: method.name.to_string(),
                span: method.span.clone(),
                methods,
                props,
                fields,
                bare_call: self.bare_call(id, method, args),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return error;
        };
        if self.names.overloads(def).len() > 1 {
            let name = format!("{}::{}", self.names.def(owner).name, method.name);
            return self.overloaded_method(id, &name, def, receiver, receiver_ty, args, expected);
        }
        let DefKind::Func(func) = self.names.def(def).kind else {
            unreachable!("methods are functions")
        };
        let (sig, type_args) = self.instantiate(func);
        if !self.on_impl_target(func, &type_args, receiver_ty, method, "method")
            || !self.check_member_there(func, receiver_ty, &method.name, method.span.clone())
        {
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return error;
        }
        self.records.methods.insert(id, def);
        self.check_visible(def, &method.span);
        self.require_bounds(func, &type_args, self.span(id));
        if !type_args.is_empty() {
            self.type_args.insert(id, type_args);
        }
        // The receiver is the first (`self`) parameter.
        match sig.params.split_first() {
            Some((&self_ty, rest)) => {
                if !self.receive(receiver, self_ty, receiver_ty) {
                    let from = self.ast.funcs[func].params[0]
                        .ty
                        .map(|t| self.ast.types.span(t).clone());
                    self.mismatch(self_ty, receiver_ty, self.span(receiver), from);
                }
                let name = format!("{}::{}", self.names.def(owner).name, method.name);
                self.check_args_from(id, &name, func, 1, rest, args);
            }
            None => {
                let name = format!("{}::{}", self.names.def(owner).name, method.name);
                self.errors.push(TypeError::WrongArgCount {
                    name,
                    expected: 0,
                    at_least: false,
                    found: args.len() + 1,
                    span: self.span(id),
                });
            }
        }
        sig.ret
    }

    /// Checks a call of one of the methods every array has, if `receiver` is an array or a
    /// reference to one, returning its type.
    pub(super) fn array_method(
        &mut self,
        call: Id<Expr>,
        receiver: Id<Expr>,
        receiver_ty: TypeId,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
        as_prop: bool,
    ) -> Option<TypeId> {
        let error = self.u.table.error;
        // `[1, 2, 3].len()`: a collection nothing else decides is an array, and a string a `str`.
        match self.u.var_kind(receiver_ty) {
            Some(VarKind::Collection { element, len }) => {
                let array = self.u.intern(Ty::Array { element, len });
                let _ = self.u.unify(receiver_ty, array);
            }
            Some(VarKind::Text) => {
                let text = self.u.table.text;
                let _ = self.u.unify(receiver_ty, text);
            }
            _ => {}
        }
        let shallow = self.u.shallow(receiver_ty);
        let (array, through) = match *self.u.get(shallow) {
            Ty::Ref { mutable, to } => (self.u.shallow(to), Some(mutable)),
            _ => (shallow, None),
        };
        let (element, is_slice) = match *self.u.get(array) {
            Ty::Array { element, .. } => (element, false),
            Ty::Slice(element) => (element, true),
            // Text is its bytes.
            Ty::Str => (self.u.table.u8, true),
            _ => return None,
        };
        let infer_args = |c: &mut Self| {
            for &arg in args {
                c.infer(arg);
            }
        };
        let is_text = *self.u.get(array) == Ty::Str;
        // `bytes` is text's own.
        let kind = ArrayMethod::from_name(&method.name).filter(|kind| is_text || !kind.text_only());
        let Some(kind) = kind else {
            // What every array has: `len`, read, and its methods.
            let methods = ArrayMethod::method_names(is_text);
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(array),
                method: method.name.to_string(),
                span: method.span.clone(),
                methods,
                props: vec![ArrayMethod::Len.name().into()],
                fields: Vec::new(),
                bare_call: self.bare_call(call, method, args),
            });
            infer_args(self);
            return Some(error);
        };
        // `len` is read as a property, `items.len`.
        if kind.is_prop() && !as_prop {
            self.errors.push(TypeError::MethodIsProp {
                ty: self.describe(array),
                name: method.name.to_string(),
                span: method.span.clone(),
                parens: args
                    .is_empty()
                    .then(|| method.span.after().merge(&self.span(call).after())),
            });
            infer_args(self);
            return Some(error);
        }
        if args.len() != kind.arity() {
            self.errors.push(TypeError::WrongArgCount {
                name: method.name.to_string(),
                expected: kind.arity(),
                at_least: false,
                found: args.len(),
                span: self.span(call),
            });
            infer_args(self);
            return Some(error);
        }
        self.records.array_methods.insert(call, kind);

        // A range of any integer type, or any integer, says which elements.
        if matches!(kind, ArrayMethod::Slice | ArrayMethod::SliceMut) {
            let int = self.u.fresh(VarKind::Int);
            let range = self.u.intern(Ty::Range(int));
            match &self.ast.exprs[args[0]] {
                // `1..` and `..2`: the ends left out are the slice's own.
                Expr::Range { start, end } if start.is_none() || end.is_none() => {
                    for &bound in start.iter().chain(end) {
                        self.expect(bound, int, None);
                    }
                    self.exprs[args[0].id] = Some(range);
                }
                _ => self.expect(args[0], range, None),
            }
        } else if let Some(&index) = args.first() {
            let int = self.u.fresh(VarKind::Int);
            self.expect(index, int, None);
        }
        if kind == ArrayMethod::Set {
            self.expect(args[1], element, None);
        }
        // Changing an element changes the array: it has to be changeable, or behind a `&mut`.
        if matches!(
            kind,
            ArrayMethod::Set | ArrayMethod::GetMut | ArrayMethod::SliceMut
        ) {
            let borrow = kind != ArrayMethod::Set;
            match through {
                Some(true) => {}
                Some(false) if *self.u.get(array) == Ty::Str => {
                    self.errors.push(TypeError::ReadOnlyStr {
                        span: self.span(receiver),
                    })
                }
                Some(false) if is_slice => self.errors.push(TypeError::ReadOnlySlice {
                    span: self.span(receiver),
                }),
                Some(false) => self.errors.push(TypeError::BehindSharedReference {
                    span: self.span(receiver),
                    borrow,
                }),
                None => self.check_mutable(receiver, borrow),
            }
        }
        if kind == ArrayMethod::Get && !self.is_copy(element) {
            self.errors.push(TypeError::NotCopyable {
                ty: self.describe(element),
                span: method.span.clone(),
                repeat: false,
            });
        }

        Some(match kind {
            // The length is a constant, usable as any integer type.
            ArrayMethod::Len => self.u.fresh(VarKind::Int),
            ArrayMethod::Get => self.u.intern(Ty::Optional(element)),
            ArrayMethod::GetRef | ArrayMethod::GetMut => {
                let reference = self.u.intern(Ty::Ref {
                    mutable: kind == ArrayMethod::GetMut,
                    to: element,
                });
                self.u.intern(Ty::Optional(reference))
            }
            ArrayMethod::Set => self.u.table.bool,
            ArrayMethod::Slice if is_text => {
                let str = self.u.table.str_ref;
                self.u.intern(Ty::Optional(str))
            }
            ArrayMethod::Slice | ArrayMethod::SliceMut => {
                let slice = self.u.intern(Ty::Slice(element));
                let view = self.u.intern(Ty::Ref {
                    mutable: kind == ArrayMethod::SliceMut,
                    to: slice,
                });
                self.u.intern(Ty::Optional(view))
            }
            ArrayMethod::Bytes => {
                let u8 = self.u.table.u8;
                let bytes = self.u.intern(Ty::Slice(u8));
                self.u.intern(Ty::Ref {
                    mutable: false,
                    to: bytes,
                })
            }
        })
    }

    /// A method call `receiver.name(args)` where several methods have that name: the one
    /// whose `self` takes the receiver, whose other parameters fit the arguments and whose
    /// result fits `expected`. Of several, the one taking the receiver as it is, then by `&`,
    /// then by `&mut`: `list.get(0)` is the `&self` one unless a `&mut` result is expected.
    #[allow(clippy::too_many_arguments)]
    fn overloaded_method(
        &mut self,
        call: Id<Expr>,
        name: &str,
        first: DefId,
        receiver: Id<Expr>,
        receiver_ty: TypeId,
        args: &[Id<Expr>],
        expected: Option<TypeId>,
    ) -> TypeId {
        let found: Vec<TypeId> = args.iter().map(|&a| self.infer(a)).collect();
        // Each fitting method, and how much the receiver has to change for it.
        let mut fitting = Vec::new();
        // One only some instances have, where this isn't one of them: not a candidate.
        let mut not_here = None;
        // If several fit numbers nothing has decided yet, `[1.0, 2.0].normalize()`, they're
        // settled to `f64` and `i32`, as they would be anyway, and it's chosen again.
        for defaulted in [false, true] {
            if defaulted {
                let mut settled = self.default_numbers(receiver_ty);
                for &ty in &found {
                    settled |= self.default_numbers(ty);
                }
                if !settled {
                    break;
                }
                fitting.clear();
            }
            // Ones the arguments fit as they are; only if there are none, ones they fit converted.
            for converting in [false, true] {
                for def in self.names.overloads(first) {
                    let DefKind::Func(func) = self.names.def(def).kind else {
                        continue;
                    };
                    if self.member_holds(func, receiver_ty) == Some(false) {
                        not_here.get_or_insert(func);
                        continue;
                    }
                    let (sig, type_args) = self.instantiate(func);
                    let Some((&self_ty, rest)) = sig.params.split_first() else {
                        continue;
                    };
                    let Some(cost) = self.receiver_cost(self_ty, receiver_ty) else {
                        continue;
                    };
                    // The receiver decides the impl's parameters, so `.read()` of an `Alloc<T>`
                    // gives `T`s, not anything. Only a receiver without unknowns, so the only
                    // variables bound are this candidate's own.
                    let (wanted, given) = (self.referent(self_ty), self.referent(receiver_ty));
                    if !self.has_unknowns(given) {
                        let _ = self.u.unify(wanted, given);
                    }
                    let fits = rest.len() == found.len()
                        && self.args_fit(rest, &found, converting)
                        && expected.is_none_or(|e| self.u.could_fit(e, sig.ret));
                    if fits {
                        fitting.push((cost, def, func, sig, type_args));
                    }
                }
                if !fitting.is_empty() {
                    break;
                }
            }
            if let Some(cheapest) = fitting.iter().map(|f| f.0).min() {
                fitting.retain(|f| f.0 == cheapest);
            }
            if fitting.len() <= 1 {
                break;
            }
        }
        let candidates = |c: &Self, defs: Vec<DefId>| -> Vec<(String, Span)> {
            (defs.into_iter())
                .filter_map(|d| match c.names.def(d).kind {
                    DefKind::Func(f) => Some((c.signature_text(f), c.names.def(d).span.clone())),
                    _ => None,
                })
                .collect()
        };
        let method_span = match &self.ast.exprs[call] {
            Expr::MethodCall { method, .. } => method.span.clone(),
            _ => self.span(call),
        };
        // Only there for some instances: none of them here, or not known which.
        let unknown = (fitting.iter()).find(|f| self.member_holds(f.2, receiver_ty).is_none());
        let only_some = match (fitting.is_empty(), unknown) {
            (true, _) => not_here,
            (false, Some(f)) if fitting.len() > 1 => Some(f.2),
            _ => None,
        };
        if let Some(func) = only_some {
            self.check_member_there(func, receiver_ty, name, method_span);
            return self.u.table.error;
        }
        if fitting.len() != 1 {
            let error = match fitting.is_empty() {
                true => TypeError::NoMatchingOverload {
                    name: name.to_string(),
                    args: std::iter::once(receiver_ty)
                        .chain(found.iter().copied())
                        .map(|t| self.describe(t))
                        .collect(),
                    candidates: candidates(self, self.names.overloads(first)),
                    span: self.span(call),
                },
                false => TypeError::AmbiguousCall {
                    name: name.to_string(),
                    candidates: candidates(self, fitting.iter().map(|f| f.1).collect()),
                    span: self.span(call),
                },
            };
            self.errors.push(error);
            return self.u.table.error;
        }
        let (_, def, func, sig, type_args) = fitting.pop().expect("exactly one");
        if !self.check_member_there(func, receiver_ty, name, method_span) {
            return self.u.table.error;
        }
        self.records.methods.insert(call, def);
        if let Expr::MethodCall { method, .. } = &self.ast.exprs[call] {
            self.check_visible(def, &method.span);
        }
        self.require_bounds(func, &type_args, self.span(call));
        if !type_args.is_empty() {
            self.type_args.insert(call, type_args);
        }
        let self_ty = sig.params[0];
        self.receive(receiver, self_ty, receiver_ty);
        for (i, ((&arg, &param), &ty)) in args.iter().zip(&sig.params[1..]).zip(&found).enumerate()
        {
            if !self.coerce(arg, param, ty) {
                let declared = &self.ast.funcs[func].params[i + 1];
                let from = declared.ty.map(|t| self.ast.types.span(t).clone());
                self.mismatch_at(arg, param, ty, from);
            }
        }
        sig.ret
    }

    /// What `ty` refers to, if it's a reference; otherwise `ty`.
    fn referent(&self, ty: TypeId) -> TypeId {
        match self.u.shallow_ty(ty) {
            Ty::Ref { to, .. } => *to,
            _ => ty,
        }
    }

    /// How much a receiver of type `found` changes to be a `self` of type `expected`, without
    /// changing anything; `None` if it can't be.
    fn receiver_cost(&self, expected: TypeId, found: TypeId) -> Option<ReceiverCost> {
        let wanted = self.u.shallow_ty(expected).clone();
        let given = self.u.shallow_ty(found).clone();
        match (wanted, given) {
            (Ty::Ref { mutable, to }, given) if !matches!(given, Ty::Ref { .. } | Ty::Error) => {
                self.u.could_fit(to, found).then_some(if mutable {
                    ReceiverCost::BorrowedMut
                } else {
                    ReceiverCost::Changed
                })
            }
            (
                Ty::Ref { mutable: m, to },
                Ty::Ref {
                    mutable: n,
                    to: given,
                },
            ) => (m == n && self.u.could_fit(to, given)).then_some(ReceiverCost::AsItIs),
            (wanted, Ty::Ref { to, .. }) if !matches!(wanted, Ty::Var(_)) => {
                (self.u.could_fit(expected, to) && self.is_copy(to))
                    .then_some(ReceiverCost::Changed)
            }
            _ => (self.u.could_fit(expected, found)).then_some(ReceiverCost::AsItIs),
        }
    }

    /// Checks a call of one of the methods every typed pointer has, if the receiver is one,
    /// returning its type.
    fn pointer_method(
        &mut self,
        call: Id<Expr>,
        receiver_ty: TypeId,
        method: &som_ast::Ident,
        args: &[Id<Expr>],
    ) -> Option<TypeId> {
        let Ty::Ptr(element) = *self.u.shallow_ty(receiver_ty) else {
            return None;
        };
        let (error, unit, bool) = (self.u.table.error, self.u.table.unit, self.u.table.bool);
        let Some(kind) = PointerMethod::from_name(&method.name) else {
            let methods = (PointerMethod::ALL.iter())
                .map(|m| m.name().to_string())
                .collect();
            self.errors.push(TypeError::UnknownMethod {
                ty: self.describe(receiver_ty),
                method: method.name.to_string(),
                span: method.span.clone(),
                methods,
                props: Vec::new(),
                fields: Vec::new(),
                bare_call: self.bare_call(call, method, args),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return Some(error);
        };
        if args.len() != kind.arity() {
            self.errors.push(TypeError::WrongArgCount {
                name: method.name.to_string(),
                expected: kind.arity(),
                at_least: false,
                found: args.len(),
                span: self.span(call),
            });
            args.iter().for_each(|&a| {
                self.infer(a);
            });
            return Some(error);
        }
        self.records.pointer_methods.insert(call, kind);
        // The first argument is a count or an index.
        let usize = self.u.table.usize;
        if let Some(&first) = args.first() {
            self.expect(first, usize, None);
        }
        Some(match kind {
            // It changes the pointer it's called on into the new one.
            PointerMethod::Resize => {
                if let Expr::MethodCall { receiver, .. } = self.ast.exprs[call] {
                    self.check_mutable(receiver, false);
                }
                unit
            }
            PointerMethod::Free => unit,
            PointerMethod::Read => element,
            PointerMethod::Write => {
                self.expect(args[1], element, None);
                unit
            }
            PointerMethod::At | PointerMethod::AtMut => self.u.intern(Ty::Ref {
                mutable: kind == PointerMethod::AtMut,
                to: element,
            }),
            PointerMethod::View | PointerMethod::ViewMut => {
                let slice = self.u.intern(Ty::Slice(element));
                self.u.intern(Ty::Ref {
                    mutable: kind == PointerMethod::ViewMut,
                    to: slice,
                })
            }
            PointerMethod::IsNull => bool,
        })
    }

    /// The struct `ty` is, looking through one reference, and its type arguments. Reports an
    /// error if it isn't one.
    /// The definition built-in type `ty`'s own methods hang on, looking through a reference;
    /// an integer not settled yet is an `i32`'s, a float an `f64`'s.
    pub(super) fn primitive_owner(&self, ty: TypeId) -> Option<DefId> {
        let mut ty = self.u.shallow(ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let primitive = match self.u.var_kind(ty) {
            Some(VarKind::Int) => som_ast::Primitive::I32,
            Some(VarKind::Float) => som_ast::Primitive::F64,
            Some(VarKind::Text) => som_ast::Primitive::Str,
            _ => crate::traits::primitive_of(self.u.get(ty))?,
        };
        self.names.primitive(primitive)
    }

    /// Whether `receiver_ty`, looking through a reference, could be what the impl block of
    /// method `func` is for, with `type_args` its instantiation. A `Vector<i32, 3>` isn't the
    /// `Vector<T, 1>` of `impl<T> Vector<T, 1>`: reported as that, before its `self` is
    /// compared and the difference gets lost in a mismatch about borrowing.
    fn on_impl_target(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
        receiver_ty: TypeId,
        name: &som_ast::Ident,
        what: &'static str,
    ) -> bool {
        if self.impl_target_fits(func, type_args, receiver_ty) {
            return true;
        }
        self.not_on_impl_target(&[func], receiver_ty, name, what);
        false
    }

    /// Reports that none of `funcs`, methods or properties called `name`, is in an impl block
    /// for `receiver_ty`, naming each block's target.
    pub(super) fn not_on_impl_target(
        &mut self,
        funcs: &[Id<Func>],
        receiver_ty: TypeId,
        name: &som_ast::Ident,
        what: &'static str,
    ) {
        let mut found = self.u.shallow(receiver_ty);
        if let Ty::Ref { to, .. } = *self.u.get(found) {
            found = self.u.shallow(to);
        }
        let targets = (funcs.iter())
            .filter_map(|&func| {
                let index = self.names.impl_of(func)?;
                let target = self.impls[index].target?;
                Some((
                    self.u.display(target, self.names),
                    self.impl_target_span(index),
                ))
            })
            .collect();
        self.errors.push(TypeError::NotOnImplTarget {
            ty: self.describe(found),
            what,
            name: name.name.to_string(),
            targets,
            span: name.span.clone(),
        });
    }

    /// Whether `receiver_ty`, looking through a reference, could be what the impl block of
    /// method `func` is for, with `type_args` its instantiation. True when that's not known.
    /// The names of `owner`'s properties a value of type `ty` has: those of the impl blocks
    /// for its type, `magnitude` of a `Vector<3, f32>`.
    /// What a value of `ty`, of struct or built-in type `owner`, has that's seen from `at`:
    /// its fields, properties and methods, by name.
    pub(super) fn members(
        &mut self,
        owner: DefId,
        ty: TypeId,
        at: &Span,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let viewer = self.viewer(at);
        let visible = |c: &Self, def: DefId| {
            visibility::hidden(c.ast, c.names, def, viewer.clone()).is_none()
        };
        let fields = match self.names.def(owner).kind {
            DefKind::Struct(s) => (self.ast.structs[s].fields.iter().enumerate())
                .filter(|&(i, _)| {
                    visibility::hidden_field(self.ast, self.names, s, i, viewer.clone()).is_none()
                })
                .map(|(_, f)| f.name.name.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let props: Vec<String> = (self.props_for(owner, ty).into_iter())
            .filter(|name| (self.names.props(owner, name).iter()).any(|&d| visible(self, d)))
            .collect();
        // Only what takes `self` can be called on a value: not `string::from`. Nor what
        // the language calls itself: `drop`, `share`, and the operators' `add` and `eq`.
        let takes_self = |c: &Self, def: DefId| match c.names.def(def).kind {
            DefKind::Func(f) => {
                let implicit = (c.names.impl_of(f))
                    .and_then(|i| c.names.impls[i].trait_)
                    .and_then(|t| c.names.lang_of(t))
                    .is_some_and(LangTrait::called_implicitly);
                !implicit
                    && (c.ast.funcs[f].params.first())
                        .is_some_and(|p| &*p.name.name == som_lang::SELF_VALUE)
            }
            _ => false,
        };
        let mut methods: Vec<String> = (self.names.methods_of(owner).into_iter())
            .filter(|&(_, def)| visible(self, def) && takes_self(self, def))
            .map(|(name, _)| name.to_string())
            .collect();
        methods.dedup();
        (fields, props, methods)
    }

    pub(super) fn impl_target_fits(
        &mut self,
        func: Id<Func>,
        type_args: &[TypeId],
        receiver_ty: TypeId,
    ) -> bool {
        let Some(index) = self.names.impl_of(func) else {
            return true;
        };
        let Some(target) = self.impls[index].target else {
            return true;
        };
        let Some(args) = type_args.get(..self.impls[index].params.len()) else {
            return true;
        };
        let subst = self.impls[index].subst(args);
        let wanted = crate::traits::substitute(&mut self.u.table, target, &subst);
        let wanted = self.u.normalize(wanted);
        // Through a reference on either side: a `str` is one already.
        let unref = |u: &Unifier, ty: TypeId| match *u.shallow_ty(ty) {
            Ty::Ref { to, .. } => u.shallow(to),
            _ => u.shallow(ty),
        };
        let found = unref(&self.u, receiver_ty);
        self.u.could_fit(unref(&self.u, wanted), found)
    }
}

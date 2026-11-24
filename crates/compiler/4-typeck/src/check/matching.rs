use som_ast::{Arm, Expr, Id, Ident, Pattern, PatternFields, Struct, VariantKind};
use som_base::message::code;
use som_base::{Fix, Span, msg};
use som_resolve::{DefId, DefKind};
use std::collections::HashSet;

use super::Checker;
use crate::error::TypeError;
use crate::exhaustive::{Ctor, Pat, useful};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;
use crate::{CommonField, FailureArms, PatternInfo};

pub(super) fn holds(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Unit => "holds nothing",
        VariantKind::Tuple => "holds values by position",
        VariantKind::Named => "has named fields",
    }
}

const MISSING_SHOWN: usize = 8;

impl Checker<'_> {
    pub(super) fn variant_fields(
        &mut self,
        s: Id<Struct>,
        index: usize,
        args: &[TypeId],
    ) -> Vec<TypeId> {
        let ast_struct = &self.ast.structs[s];
        let start = ast_struct.payload_start(index);
        let count = ast_struct.variants()[index].fields.len();
        let subst = self.struct_subst(s, args);
        (start..start + count)
            .map(|i| self.substitute(self.records.structs[s.id][i], &subst))
            .collect()
    }

    pub(super) fn variant_name(&self, s: Id<Struct>, index: usize) -> String {
        let ast_struct = &self.ast.structs[s];
        format!("{}::{}", ast_struct.name, ast_struct.variants()[index].name)
    }

    pub(super) fn variant_form(&self, s: Id<Struct>, index: usize) -> String {
        let name = self.variant_name(s, index);
        match self.ast.structs[s].variants()[index].kind {
            VariantKind::Unit => name,
            VariantKind::Tuple => format!("{name}(…)"),
            VariantKind::Named => format!("{name} {{ … }}"),
        }
    }

    /// Its enum's type (a fresh variable per type argument), the enum, the variant index, and
    /// its field types.
    pub(super) fn variant_value(
        &mut self,
        variant: DefId,
    ) -> (TypeId, Id<Struct>, usize, Vec<TypeId>) {
        let DefKind::Variant { strukt, index } = self.names.def(variant).kind else {
            unreachable!("only called for variants")
        };
        let def = self.names.strukt(strukt);
        let args: Vec<TypeId> = (self.names.struct_params(strukt).iter())
            .map(|_| self.u.fresh(VarKind::Any))
            .collect();
        let fields = self.variant_fields(strukt, index, &args);
        let ty = self.u.intern(Ty::Struct(def, args));
        (ty, strukt, index, fields)
    }

    fn enum_of(&self, ty: TypeId) -> Option<(Id<Struct>, Vec<TypeId>)> {
        match self.u.shallow_ty(ty) {
            Ty::Struct(def, args) => match self.names.def(*def).kind {
                DefKind::Struct(s) if self.ast.structs[s].is_enum() => Some((s, args.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    pub(super) fn common_fields(&self, id: Id<Struct>, s: &Struct) -> Vec<CommonField> {
        let Some(first) = s.variants().first() else {
            return Vec::new();
        };
        let payload = &self.records.structs[id.id];
        let mut common = Vec::new();
        for (k, field) in first.fields.iter().enumerate() {
            let ty = payload[s.payload_start(0) + k];
            let positions: Option<Vec<usize>> = (s.variants().iter().enumerate())
                .map(|(i, v)| {
                    if v.kind != VariantKind::Named {
                        return None;
                    }
                    let j = v
                        .fields
                        .iter()
                        .position(|f| f.name.name == field.name.name)?;
                    let at = s.payload_start(i) + j;
                    (payload[at] == ty).then_some(at)
                })
                .collect();
            if let Some(payload) = positions {
                common.push(CommonField {
                    name: field.name.name.clone(),
                    payload,
                });
            }
        }
        common
    }

    pub(super) fn common_field(
        &mut self,
        expr: Id<Expr>,
        s: Id<Struct>,
        args: &[TypeId],
        field: &Ident,
    ) -> TypeId {
        let common = &self.records.common[s.id];
        if let Some(k) = common.iter().position(|c| c.name == field.name) {
            self.records.fields.insert(expr, k);
            let at = common[k].payload[0];
            let subst = self.struct_subst(s, args);
            return self.substitute(self.records.structs[s.id][at], &subst);
        }
        let ast_struct = &self.ast.structs[s];
        let without: Vec<String> = (ast_struct.variants().iter())
            .filter(|v| !v.fields.iter().any(|f| f.name.name == field.name))
            .map(|v| v.name.name.to_string())
            .collect();
        let error = if without.len() < ast_struct.variants().len() {
            // Some variants have it: only a `match` knows which one a value is.
            TypeError::NotInEveryVariant {
                ty: ast_struct.name.name.to_string(),
                field: field.name.to_string(),
                without,
                span: field.span.clone(),
            }
        } else {
            TypeError::UnknownField {
                ty: ast_struct.name.name.to_string(),
                field: field.name.to_string(),
                available: common.iter().map(|c| c.name.to_string()).collect(),
                props: Vec::new(),
                methods: Vec::new(),
                span: field.span.clone(),
            }
        };
        self.errors.push(error);
        self.u.table.error
    }

    /// With `discarded`, the value isn't used, so the arms can differ in type.
    pub(super) fn check_match(
        &mut self,
        id: Id<Expr>,
        scrutinee: Id<Expr>,
        arms: &[Arm],
        discarded: bool,
    ) -> TypeId {
        let found = self.infer(scrutinee);
        // A collection nothing else decides, as in `for x in [1, 2]`, is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(found) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(found, array);
        }
        // Through a reference, what it points to is matched, and bindings borrow from it.
        let (matched, by_ref) = match *self.u.shallow_ty(found) {
            Ty::Ref { mutable, to } => (to, Some(mutable)),
            _ => (found, None),
        };

        // An overloaded call not decided yet: pick the function whose result the arms can
        // match. `"yes"` matches text or an optional's text; `none` only an optional.
        if self.u.var_kind(matched) == Some(VarKind::Any) {
            let has = |f: fn(&Pattern) -> bool| {
                (arms.iter()).any(|arm| f(&self.ast.patterns[arm.pattern]))
            };
            let none = has(|p| matches!(p, Pattern::None));
            let text = has(|p| matches!(p, Pattern::Str(_)));
            let int = has(|p| matches!(p, Pattern::Int(_)));
            self.decide_pending_by(matched, &|c, ret| {
                let ret = c.u.shallow(ret);
                let (optional, value) = match *c.u.get(ret) {
                    Ty::Optional(inner) => (true, c.u.shallow(inner)),
                    _ => (false, ret),
                };
                let value_ty = c.u.get(value);
                (optional || !none)
                    && (!text || value == c.u.table.text || matches!(value_ty, Ty::CStr))
                    && (!int || matches!(value_ty, Ty::Int(_)))
            });
        }

        // Still undecided with a `none` arm: an optional, so a `"yes"` before it matches the
        // held value rather than deciding the type is text.
        if self.u.var_kind(matched).is_some()
            && (arms.iter()).any(|arm| matches!(self.ast.patterns[arm.pattern], Pattern::None))
        {
            let inner = self.u.fresh(VarKind::Any);
            let optional = self.u.intern(Ty::Optional(inner));
            let _ = self.u.unify(matched, optional);
        }

        let outer: HashSet<usize> = self.narrowed.iter().map(|n| n.id).collect();
        let mut result: Option<(TypeId, Id<Expr>)> = None;
        let mut valued = Vec::new();
        let mut rows = Vec::new();
        let mut none_matched = false;
        for arm in arms {
            let errors = self.errors.len();
            let pat = self.check_pattern(arm.pattern, matched, by_ref, none_matched);
            let ok = self.errors.len() == errors;
            rows.push(ok.then_some(pat));
            none_matched |= matches!(self.ast.patterns[arm.pattern], Pattern::None);

            let ty = self.infer(arm.body);
            // What an arm narrowed doesn't hold in the others, or after the `match`.
            self.narrowed.retain(|n| outer.contains(&n.id));
            if self.expr_leaves(arm.body) {
                self.records.diverging.insert(arm.body);
                continue;
            }
            if discarded {
                continue;
            }
            match result {
                None => result = Some((ty, arm.body)),
                Some((expected, first)) => {
                    if self.coerce(arm.body, expected, ty) {
                        // Fine as it is.
                    } else if let Ty::Optional(inner) = *self.u.shallow_ty(ty)
                        && self.u.unify(inner, expected).is_ok()
                    {
                        // `none` after values: they're all optionals, the values wrapped.
                        self.records.wrapped.extend(valued.iter().copied());
                        result = Some((ty, first));
                    } else {
                        let from = self.span(first);
                        self.mismatch(expected, ty, self.span(arm.body), Some(from));
                    }
                }
            }
            valued.push(arm.body);
        }

        if rows.iter().all(Option::is_some) && *self.u.shallow_ty(matched) != Ty::Error {
            let pats: Vec<Pat> = rows.into_iter().flatten().collect();
            // A `T!`'s failures are checked once what it can fail with is worked out: here,
            // only the values it can hold.
            let assumed = match self.outcome_inner(matched) {
                Some(_) => {
                    let arms_found = self.failure_arms_of(arms, &pats);
                    self.records.failure_arms.insert(id, arms_found);
                    vec![Pat::Ctor(Ctor::Variant(1), vec![Pat::Wild])]
                }
                None => Vec::new(),
            };
            self.check_coverage(id, arms, &pats, matched, &assumed);
        }

        match result {
            _ if discarded => self.u.table.unit,
            Some((ty, _)) => ty,
            // Every arm leaves: there's never a value, so it fits wherever it's used.
            None if !arms.is_empty() => self.u.fresh(VarKind::Any),
            None => self.u.table.unit,
        }
    }

    /// `assumed`: patterns taken to match, for what's missing, though no arm has them.
    fn check_coverage(
        &mut self,
        id: Id<Expr>,
        arms: &[Arm],
        pats: &[Pat],
        matched: TypeId,
        assumed: &[Pat],
    ) {
        let (unreachable, missing) = {
            let mut space = |ty: TypeId| self.ctors(ty);
            let mut seen: Vec<Vec<Pat>> = Vec::new();
            let mut unreachable = Vec::new();
            for (i, pat) in pats.iter().enumerate() {
                let row = vec![pat.clone()];
                if useful(&seen, &row, &[matched], &mut space).is_none() {
                    unreachable.push(i);
                }
                seen.push(row);
            }
            seen.extend(assumed.iter().map(|p| vec![p.clone()]));
            let mut missing = Vec::new();
            while missing.len() < MISSING_SHOWN
                && let Some(witness) = useful(&seen, &[Pat::Wild], &[matched], &mut space)
            {
                seen.push(witness.clone());
                missing.extend(witness);
            }
            (unreachable, missing)
        };

        for i in unreachable {
            let earlier = (pats[..i].iter().zip(arms))
                .find(|(pat, _)| matches!(pat, Pat::Wild))
                .map(|(_, arm)| self.ast.patterns.span(arm.pattern).clone());
            self.errors.push(TypeError::UnreachableArm {
                span: self.ast.patterns.span(arms[i].pattern).clone(),
                earlier,
            });
        }
        if missing.is_empty() {
            return;
        }
        let missing: Vec<String> = missing.iter().map(|w| self.render(w, matched)).collect();
        let fix = self.missing_arms_fix(id, &missing);
        let span = match &self.ast.exprs[id] {
            Expr::Match { scrutinee, .. } => self.span(*scrutinee),
            _ => self.span(id),
        };
        self.errors.push(TypeError::NonExhaustiveMatch {
            ty: self.describe(matched),
            missing,
            span,
            fix,
        });
    }

    /// What the arms of a `match` on a `T!` say of failures: whether one takes any, and for
    /// each struct or enum with arms of its own, whether they take all of its values.
    fn failure_arms_of(&mut self, arms: &[Arm], pats: &[Pat]) -> FailureArms {
        let mut found = FailureArms::default();
        let mut rows: Vec<(DefId, Id<Pattern>, Vec<Vec<Pat>>)> = Vec::new();
        for (arm, pat) in arms.iter().zip(pats) {
            match pat {
                Pat::Wild => found.catch_all = true,
                Pat::Ctor(Ctor::Variant(1), inner) => match inner.first() {
                    Some(Pat::Ctor(Ctor::Kind(def), value)) => {
                        let value = value.first().cloned().unwrap_or(Pat::Wild);
                        match rows.iter_mut().find(|(d, ..)| d == def) {
                            Some((_, _, of)) => of.push(vec![value]),
                            None => rows.push((*def, arm.pattern, vec![vec![value]])),
                        }
                    }
                    _ => found.catch_all = true,
                },
                _ => {}
            }
        }
        for (def, first, mut of) in rows {
            let ty = self.u.intern(Ty::Struct(def, Vec::new()));
            let mut missing = Vec::new();
            loop {
                let mut space = |ty: TypeId| self.ctors(ty);
                let Some(witness) = useful(&of, &[Pat::Wild], &[ty], &mut space) else {
                    break;
                };
                if missing.len() >= MISSING_SHOWN {
                    break;
                }
                let name = &self.names.def(def).name;
                let shown = match &witness[0] {
                    Pat::Wild => format!("{name} {{ .. }}"),
                    // A variant, named with its enum's name: nothing says which enum it is.
                    pat @ Pat::Ctor(Ctor::Variant(_), _) => format!("{name}::{}", self.render(pat, ty)),
                    pat => self.render(pat, ty),
                };
                missing.push(format!("fail {shown}"));
                of.push(witness);
            }
            found.kinds.push((def, missing.is_empty(), first));
            if !missing.is_empty() {
                found.missing.push((def, missing));
            }
        }
        found
    }

    fn missing_arms_fix(&self, id: Id<Expr>, missing: &[String]) -> Option<Fix> {
        let span = self.span(id);
        let text = span.source.content();
        let close = span.start_offset + span.length.checked_sub(1)?;
        if text.as_bytes().get(close) != Some(&b'}') {
            return None;
        }
        // From just after what comes before the `}`, so a missing comma can go there.
        let before = text[..close].trim_end();
        let needs_comma = !before.ends_with([',', '{', '}']);
        let comma = if needs_comma { "," } else { "" };
        let line_start = text[..close].rfind('\n').map_or(0, |i| i + 1);
        let indent = &text[line_start..close];
        let arms: Vec<String> = missing.iter().map(|m| format!("{m} => {{}}")).collect();
        let replacement = if indent.trim().is_empty() {
            let inner = format!("{indent}    ");
            let arms = arms.join(&format!(",\n{inner}"));
            format!("{comma}\n{inner}{arms},\n{indent}}}")
        } else {
            format!("{comma} {} }}", arms.join(", "))
        };
        let range = before.len()..close + 1;
        let title = if missing.len() == 1 {
            msg!["add an arm for ", code(&missing[0])]
        } else {
            msg!["add the missing arms"]
        };
        Some(Fix::replace(
            title,
            Span::from_range(range, span.source.clone()),
            replacement,
        ))
    }

    fn ctors(&mut self, ty: TypeId) -> Option<Vec<(Ctor, Vec<TypeId>)>> {
        match self.u.shallow_ty(ty).clone() {
            Ty::Bool => Some(vec![
                (Ctor::Bool(false), vec![]),
                (Ctor::Bool(true), vec![]),
            ]),
            Ty::Optional(inner) => Some(vec![(Ctor::None, vec![]), (Ctor::Some, vec![inner])]),
            _ => {
                let (s, args) = self.enum_of(ty)?;
                let count = self.ast.structs[s].variants().len();
                Some(
                    (0..count)
                        .map(|i| (Ctor::Variant(i), self.variant_fields(s, i, &args)))
                        .collect(),
                )
            }
        }
    }

    fn render(&mut self, pat: &Pat, ty: TypeId) -> String {
        match pat {
            Pat::Wild => "_".into(),
            Pat::Ctor(Ctor::Bool(b), _) => b.to_string(),
            Pat::Ctor(Ctor::Int(n), _) => n.to_string(),
            Pat::Ctor(Ctor::Str(s), _) => format!("{s:?}"),
            Pat::Ctor(Ctor::None, _) => "none".into(),
            Pat::Ctor(Ctor::Kind(def), _) => format!("{} {{ .. }}", self.names.def(*def).name),
            // A `T!`'s: a pattern for the `T`, or `fail …`.
            Pat::Ctor(Ctor::Variant(0), inner) if let Some(ok) = self.outcome_inner(ty) => {
                self.render(&inner[0], ok)
            }
            Pat::Ctor(Ctor::Variant(1), inner) if self.outcome_inner(ty).is_some() => {
                match inner.first() {
                    Some(Pat::Ctor(Ctor::Kind(def), _)) => {
                        format!("fail {} {{ .. }}", self.names.def(*def).name)
                    }
                    _ => "fail e".into(),
                }
            }
            // A pattern for an optional's value is written as one for the value.
            Pat::Ctor(Ctor::Some, inner) => match *self.u.shallow_ty(ty) {
                Ty::Optional(value) => self.render(&inner[0], value),
                _ => "_".into(),
            },
            Pat::Ctor(Ctor::Variant(index), fields) => {
                let Some((s, args)) = self.enum_of(ty) else {
                    return "_".into();
                };
                let field_tys = self.variant_fields(s, *index, &args);
                let parts: Vec<String> = (fields.iter().zip(field_tys))
                    .map(|(f, ty)| self.render(f, ty))
                    .collect();
                let variant = &self.ast.structs[s].variants()[*index];
                let name = &variant.name.name;
                match variant.kind {
                    VariantKind::Unit => name.to_string(),
                    VariantKind::Tuple => format!("{name}({})", parts.join(", ")),
                    VariantKind::Named => {
                        let named: Vec<String> = (variant.fields.iter().zip(&parts))
                            .filter(|(_, p)| *p != "_")
                            .map(|(f, p)| format!("{}: {p}", f.name))
                            .collect();
                        if named.is_empty() {
                            format!("{name} {{ .. }}")
                        } else if named.len() == parts.len() {
                            format!("{name} {{ {} }}", named.join(", "))
                        } else {
                            format!("{name} {{ {}, .. }}", named.join(", "))
                        }
                    }
                }
            }
        }
    }

    /// `by_ref`: matched through a reference (`&mut` or not), so bindings borrow.
    /// `none_matched`: an earlier arm matched `none`, so a binding binds the value an optional
    /// holds.
    pub(super) fn check_pattern(
        &mut self,
        id: Id<Pattern>,
        ty: TypeId,
        by_ref: Option<bool>,
        none_matched: bool,
    ) -> Pat {
        let ast = self.ast;
        let error = self.u.table.error;
        let span = ast.patterns.span(id).clone();
        let mut info = PatternInfo::default();
        let shallow = self.u.shallow(ty);
        // Something wrong with the type was already reported: bind names, say nothing more.
        if *self.u.get(shallow) == Ty::Error {
            self.bind_all(id, error);
            return Pat::Wild;
        }
        // Against a `T!`: `fail …`, or a pattern for the `T`.
        if let Some(ok) = self.outcome_inner(shallow)
            && !self.names_outcome_variant(id)
        {
            return self.outcome_pattern(id, ok, by_ref, none_matched);
        }
        let optional = match *self.u.get(shallow) {
            Ty::Optional(inner) => Some(inner),
            _ => None,
        };

        // Rust's `Some(v)` and `None`, against an optional of something without variants of
        // those names: som matches the value itself, and `none`.
        if let Some(inner) = optional
            && let Some(error) = self.rust_optional_pattern(id, inner, none_matched)
        {
            self.errors.push(error);
            self.bind_all(id, error_ty(self));
            self.records.patterns.insert(id, info);
            return Pat::Wild;
        }
        let pat = match &ast.patterns[id] {
            Pattern::Wildcard => Pat::Wild,
            // A name that's a variant, `Empty`; or looks like one, against an enum, where a new
            // name would quietly match everything: `Emtpy`.
            Pattern::Binding { name, .. }
                if self.names.pattern(id).is_none()
                    || self
                        .variant_index(optional.unwrap_or(ty), &name.name)
                        .is_some()
                    || (name.name.starts_with(|c: char| c.is_ascii_uppercase())
                        && self.enum_of(optional.unwrap_or(ty)).is_some()) =>
            {
                self.unwrapped(ty, optional, &mut info, |c, ty, info| {
                    c.variant_pattern(id, ty, name, &PatternFields::Unit, by_ref, info)
                })
            }
            Pattern::Binding { .. } => {
                let def = self.names.pattern(id).expect("checked above");
                let (bound, unwraps) = match optional {
                    Some(inner) if none_matched => (inner, true),
                    _ => (ty, false),
                };
                info.unwraps = unwraps;
                // Through `&mut`, a reference to change it with; through `&`, one to anything
                // that can't be copied out.
                let bound = match by_ref {
                    Some(mutable) if mutable || !self.is_copy(bound) => {
                        info.by_ref = Some(mutable);
                        self.u.intern(Ty::Ref { mutable, to: bound })
                    }
                    _ => bound,
                };
                self.locals.insert(def, bound);
                match unwraps {
                    true => Pat::Ctor(Ctor::Some, vec![Pat::Wild]),
                    false => Pat::Wild,
                }
            }
            Pattern::Int(n) => self.unwrapped(ty, optional, &mut info, |c, ty, _| {
                let int = c.u.fresh(VarKind::Int);
                if c.u.unify(ty, int).is_err() {
                    c.pattern_mismatch(id, ty);
                }
                Pat::Ctor(Ctor::Int(*n), Vec::new())
            }),
            Pattern::Str(s) => self.unwrapped(ty, optional, &mut info, |c, ty, _| {
                let text = c.u.fresh(VarKind::Text);
                if c.u.unify(ty, text).is_err() {
                    c.pattern_mismatch(id, ty);
                }
                Pat::Ctor(Ctor::Str(s.clone()), Vec::new())
            }),
            Pattern::Bool(b) => self.unwrapped(ty, optional, &mut info, |c, ty, _| {
                let bool = c.u.table.bool;
                if c.u.unify(ty, bool).is_err() {
                    c.pattern_mismatch(id, ty);
                }
                Pat::Ctor(Ctor::Bool(*b), Vec::new())
            }),
            Pattern::None => {
                let inner = self.u.fresh(VarKind::Any);
                let optional = self.u.intern(Ty::Optional(inner));
                if self.u.unify(ty, optional).is_err() {
                    self.pattern_mismatch(id, ty);
                }
                Pat::Ctor(Ctor::None, Vec::new())
            }
            // `NotFound { path }`: a struct's fields, as after `fail`.
            Pattern::Variant { fields, .. }
                if let Some(def) = self.names.pattern(id)
                    && let DefKind::Struct(s) = self.names.def(def).kind
                    && !self.ast.structs[s].is_enum() =>
            {
                self.unwrapped(ty, optional, &mut info, |c, ty, info| {
                    c.struct_pattern(id, ty, s, fields, by_ref, info)
                })
            }
            Pattern::Variant { path, fields } => {
                self.unwrapped(ty, optional, &mut info, |c, ty, info| {
                    c.variant_pattern(id, ty, path.name(), fields, by_ref, info)
                })
            }
            // Only a `T!` fails.
            Pattern::Failed(inner) => {
                self.pattern_mismatch(id, ty);
                self.bind_all(*inner, error);
                Pat::Wild
            }
        };
        let _ = span;
        self.records.patterns.insert(id, info);
        pat
    }

    /// Patterns other than `none` and bindings match the value an `optional` holds, so `check`
    /// runs on that.
    fn unwrapped(
        &mut self,
        ty: TypeId,
        optional: Option<TypeId>,
        info: &mut PatternInfo,
        check: impl FnOnce(&mut Self, TypeId, &mut PatternInfo) -> Pat,
    ) -> Pat {
        match optional {
            Some(inner) => {
                info.unwraps = true;
                let pat = check(self, inner, info);
                Pat::Ctor(Ctor::Some, vec![pat])
            }
            None => check(self, ty, info),
        }
    }

    fn variant_index(&self, ty: TypeId, name: &str) -> Option<usize> {
        let (s, _) = self.enum_of(ty)?;
        (self.ast.structs[s].variants().iter()).position(|v| &*v.name.name == name)
    }

    fn variant_pattern(
        &mut self,
        id: Id<Pattern>,
        ty: TypeId,
        name: &Ident,
        fields: &PatternFields,
        by_ref: Option<bool>,
        info: &mut PatternInfo,
    ) -> Pat {
        let error = self.u.table.error;
        let Some((s, args)) = self.enum_of(ty) else {
            self.pattern_mismatch(id, ty);
            self.bind_fields(fields, error);
            return Pat::Wild;
        };
        // Which variant: the one a qualified name names, or the enum's of that name.
        let index = match self.names.pattern(id).map(|d| &self.names.def(d).kind) {
            Some(DefKind::Variant { strukt, index }) if *strukt == s => Some(*index),
            Some(DefKind::Variant { .. } | DefKind::Func(_)) => {
                self.pattern_mismatch(id, ty);
                self.bind_fields(fields, error);
                return Pat::Wild;
            }
            _ => self.variant_index(ty, &name.name),
        };
        let Some(index) = index else {
            let ast_struct = &self.ast.structs[s];
            self.errors.push(TypeError::UnknownVariant {
                ty: ast_struct.name.name.to_string(),
                variant: name.name.to_string(),
                available: (ast_struct.variants().iter())
                    .map(|v| v.name.name.to_string())
                    .collect(),
                span: name.span.clone(),
            });
            self.bind_fields(fields, error);
            return Pat::Wild;
        };
        info.variant = Some(index);

        let variant = &self.ast.structs[s].variants()[index];
        let field_tys = self.variant_fields(s, index, &args);
        let mut subs = vec![Pat::Wild; field_tys.len()];
        match (fields, variant.kind) {
            (PatternFields::Unit, VariantKind::Unit) => {}
            (PatternFields::Tuple(pats), VariantKind::Tuple) => {
                if pats.len() != field_tys.len() {
                    self.errors.push(TypeError::PatternFieldCount {
                        variant: variant.name.name.to_string(),
                        expected: field_tys.len(),
                        found: pats.len(),
                        span: self.ast.patterns.span(id).clone(),
                    });
                    self.bind_fields(fields, error);
                } else {
                    for (i, &pat) in pats.iter().enumerate() {
                        subs[i] = self.check_pattern(pat, field_tys[i], by_ref, false);
                        info.fields.push(i);
                    }
                }
            }
            (
                PatternFields::Named {
                    fields: given,
                    rest,
                },
                VariantKind::Named,
            ) => {
                let mut seen = vec![false; field_tys.len()];
                for (field, pat) in given {
                    match variant
                        .fields
                        .iter()
                        .position(|f| f.name.name == field.name)
                    {
                        Some(i) if !seen[i] => {
                            seen[i] = true;
                            subs[i] = self.check_pattern(*pat, field_tys[i], by_ref, false);
                            info.fields.push(i);
                        }
                        Some(_) => {
                            self.errors.push(TypeError::DuplicateField {
                                field: field.name.to_string(),
                                span: field.span.clone(),
                            });
                            self.bind_all(*pat, error);
                            info.fields.push(usize::MAX);
                        }
                        None => {
                            self.errors.push(TypeError::UnknownField {
                                ty: self.variant_name(s, index),
                                field: field.name.to_string(),
                                available: (variant.fields.iter())
                                    .map(|f| f.name.name.to_string())
                                    .collect(),
                                props: Vec::new(),
                                methods: Vec::new(),
                                span: field.span.clone(),
                            });
                            self.bind_all(*pat, error);
                            info.fields.push(usize::MAX);
                        }
                    }
                }
                let missing: Vec<String> = (variant.fields.iter().zip(&seen))
                    .filter(|(_, seen)| !**seen)
                    .map(|(f, _)| f.name.name.to_string())
                    .collect();
                if !rest && !missing.is_empty() {
                    self.errors.push(TypeError::MissingFieldPatterns {
                        variant: variant.name.name.to_string(),
                        fields: missing,
                        span: self.ast.patterns.span(id).clone(),
                    });
                }
            }
            (_, kind) => {
                let name = &variant.name.name;
                let form = match kind {
                    VariantKind::Unit => name.to_string(),
                    VariantKind::Tuple => {
                        let wilds = vec!["_"; variant.fields.len()];
                        format!("{name}({})", wilds.join(", "))
                    }
                    VariantKind::Named => format!("{name} {{ .. }}"),
                };
                self.errors.push(TypeError::WrongVariantForm {
                    name: self.variant_name(s, index),
                    holds: holds(kind),
                    form,
                    span: self.ast.patterns.span(id).clone(),
                });
                self.bind_fields(fields, error);
            }
        }
        Pat::Ctor(Ctor::Variant(index), subs)
    }

    /// The error for Rust's `Some(v)` or `None` against an optional: som matches the held value
    /// with the pattern itself, `v`, after `none`.
    fn rust_optional_pattern(
        &self,
        id: Id<Pattern>,
        inner: TypeId,
        none_matched: bool,
    ) -> Option<TypeError> {
        let span = self.ast.patterns.span(id).clone();
        let name = match &self.ast.patterns[id] {
            Pattern::Binding { name, .. } => &name.name,
            Pattern::Variant { path, .. } => &path.name().name,
            _ => return None,
        };
        if self.variant_index(inner, name).is_some() {
            return None;
        }
        match &self.ast.patterns[id] {
            Pattern::Binding { name, .. } if &*name.name == "None" => {
                Some(TypeError::RustOptionalPattern {
                    written: "None".into(),
                    span: span.clone(),
                    fix: Some(Fix::change_to(span, "none")),
                    needs_none_first: false,
                })
            }
            Pattern::Variant {
                path,
                fields: PatternFields::Tuple(subs),
            } if path.segments.len() == 1 && &*path.name().name == "Some" && subs.len() == 1 => {
                let sub = subs[0];
                let text = self.ast.patterns.span(sub).get_text().to_string();
                // A name binds the value only once `none` is matched: before it, the whole
                // optional.
                let binding = matches!(self.ast.patterns[sub], Pattern::Binding { .. });
                let fix =
                    (!binding || none_matched).then(|| Fix::change_to(span.clone(), text.clone()));
                Some(TypeError::RustOptionalPattern {
                    written: self.ast.pattern_text(id),
                    span,
                    fix,
                    needs_none_first: binding && !none_matched,
                })
            }
            _ => None,
        }
    }

    fn pattern_mismatch(&mut self, id: Id<Pattern>, ty: TypeId) {
        self.errors.push(TypeError::PatternMismatch {
            pattern: self.ast.pattern_text(id),
            ty: self.describe(ty),
            span: self.ast.patterns.span(id).clone(),
        });
    }

    /// Gives every name the patterns in `fields` bind type `ty`, after an error.
    fn bind_fields(&mut self, fields: &PatternFields, ty: TypeId) {
        match fields {
            PatternFields::Unit => {}
            PatternFields::Tuple(pats) => pats.iter().for_each(|&p| self.bind_all(p, ty)),
            PatternFields::Named { fields, .. } => {
                fields.iter().for_each(|&(_, p)| self.bind_all(p, ty))
            }
        }
    }

    /// Gives every name pattern `id` binds type `ty`, after an error.
    pub(super) fn bind_all(&mut self, id: Id<Pattern>, ty: TypeId) {
        self.records.patterns.entry(id).or_default();
        match &self.ast.patterns[id] {
            Pattern::Binding { .. } => {
                if let Some(def) = self.names.pattern(id)
                    && matches!(self.names.def(def).kind, DefKind::Binding(_))
                {
                    self.locals.insert(def, ty);
                }
            }
            Pattern::Variant { fields, .. } => self.bind_fields(fields, ty),
            Pattern::Failed(inner) => self.bind_all(*inner, ty),
            _ => {}
        }
    }

    /// `NotFound { path }`: struct `s`'s fields, each by name, `..` leaving the rest.
    fn struct_pattern(
        &mut self,
        id: Id<Pattern>,
        ty: TypeId,
        s: Id<Struct>,
        fields: &PatternFields,
        by_ref: Option<bool>,
        info: &mut PatternInfo,
    ) -> Pat {
        let error = self.u.table.error;
        let def = self.names.strukt(s);
        let strukt = &self.ast.structs[s];
        // Its type arguments, whatever `ty` says they are.
        let args: Vec<TypeId> = (0..strukt.generics.len()).map(|_| self.u.fresh(VarKind::Any)).collect();
        let wanted = self.u.intern(Ty::Struct(def, args.clone()));
        if self.u.unify(ty, wanted).is_err() {
            self.pattern_mismatch(id, ty);
            self.bind_fields(fields, error);
            return Pat::Wild;
        }
        let subst = self.struct_subst(s, &args);
        let field_tys: Vec<TypeId> = (0..strukt.fields.len())
            .map(|i| self.substitute(self.records.structs[s.id][i], &subst))
            .collect();
        let PatternFields::Named { fields: given, rest } = fields else {
            if !matches!(fields, PatternFields::Unit) || !strukt.fields.is_empty() {
                self.errors.push(TypeError::WrongVariantForm {
                    name: strukt.name.name.to_string(),
                    holds: "fields",
                    form: format!("{} {{ .. }}", strukt.name.name),
                    span: self.ast.patterns.span(id).clone(),
                });
                self.bind_fields(fields, error);
            }
            return Pat::Wild;
        };
        let mut seen = vec![false; field_tys.len()];
        let mut refutable = false;
        for (field, pat) in given {
            match strukt.fields.iter().position(|f| f.name.name == field.name) {
                Some(i) if !seen[i] => {
                    seen[i] = true;
                    let sub = self.check_pattern(*pat, field_tys[i], by_ref, false);
                    refutable |= !matches!(sub, Pat::Wild);
                    info.fields.push(i);
                }
                found => {
                    match found {
                        Some(_) => self.errors.push(TypeError::DuplicateField {
                            field: field.name.to_string(),
                            span: field.span.clone(),
                        }),
                        None => self.errors.push(TypeError::UnknownField {
                            ty: strukt.name.name.to_string(),
                            field: field.name.to_string(),
                            available: (strukt.fields.iter()).map(|f| f.name.name.to_string()).collect(),
                            props: Vec::new(),
                            methods: Vec::new(),
                            span: field.span.clone(),
                        }),
                    }
                    self.bind_all(*pat, error);
                    info.fields.push(usize::MAX);
                }
            }
        }
        let missing: Vec<String> = (strukt.fields.iter().zip(&seen))
            .filter(|(_, seen)| !**seen)
            .map(|(f, _)| f.name.name.to_string())
            .collect();
        if !rest && !missing.is_empty() {
            self.errors.push(TypeError::MissingFieldPatterns {
                variant: strukt.name.name.to_string(),
                fields: missing,
                span: self.ast.patterns.span(id).clone(),
            });
        }
        // Names for its fields match any; anything more might not, which isn't followed
        // further: it's taken to leave some values.
        match refutable {
            false => Pat::Wild,
            true => Pat::Ctor(Ctor::Kind(def), Vec::new()),
        }
    }
}

fn error_ty(c: &Checker) -> TypeId {
    c.u.table.error
}

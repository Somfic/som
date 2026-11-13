//! Enums and `match`: making a variant, checking each arm's pattern against the type matched,
//! and making sure the arms cover every value.

use som_ast::{Arm, Expr, Id, Ident, Pattern, PatternFields, Struct, VariantKind};
use som_base::{Fix, Span, msg};
use som_base::message::code;
use som_resolve::{DefId, DefKind};
use std::collections::HashSet;

use super::Checker;
use crate::PatternInfo;
use crate::error::TypeError;
use crate::exhaustive::{Ctor, Pat, useful};
use crate::ty::{Ty, TypeId};
use crate::unify::VarKind;

/// What a variant of `kind` holds, for messages: "`Shape::Circle` holds values by position".
pub(super) fn holds(kind: VariantKind) -> &'static str {
    match kind {
        VariantKind::Unit => "holds nothing",
        VariantKind::Tuple => "holds values by position",
        VariantKind::Named => "has named fields",
    }
}

impl Checker<'_> {
    // --- Variants ---

    /// The fields of enum `s`'s variant `index`, with its type arguments `args` filled in.
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
            .map(|i| self.substitute(self.structs[s.id][i], &subst))
            .collect()
    }

    /// `Shape::Circle`
    pub(super) fn variant_name(&self, s: Id<Struct>, index: usize) -> String {
        let ast_struct = &self.ast.structs[s];
        format!("{}::{}", ast_struct.name, ast_struct.variants()[index].name)
    }

    /// How a value of variant `index` of `s` is made: `Shape::Circle(…)`, `Shape::Rect { … }`
    /// or `Shape::Empty`.
    pub(super) fn variant_form(&self, s: Id<Struct>, index: usize) -> String {
        let name = self.variant_name(s, index);
        match self.ast.structs[s].variants()[index].kind {
            VariantKind::Unit => name,
            VariantKind::Tuple => format!("{name}(…)"),
            VariantKind::Named => format!("{name} {{ … }}"),
        }
    }

    /// A value of `variant`: its enum's type, with a fresh variable for each type argument; the
    /// enum; which variant it is; and the types of its fields.
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

    /// The enum `ty` is, and its type arguments.
    fn enum_of(&self, ty: TypeId) -> Option<(Id<Struct>, Vec<TypeId>)> {
        match self.u.get(self.u.shallow(ty)) {
            Ty::Struct(def, args) => match self.names.def(*def).kind {
                DefKind::Struct(s) if self.ast.structs[s].is_enum() => Some((s, args.clone())),
                _ => None,
            },
            _ => None,
        }
    }

    // --- `match` ---

    /// `match scrutinee { arms }`. With `discarded`, its value isn't used, so the arms can be of
    /// different types.
    pub(super) fn check_match(
        &mut self,
        id: Id<Expr>,
        scrutinee: Id<Expr>,
        arms: &[Arm],
        discarded: bool,
    ) -> TypeId {
        let found = self.infer(scrutinee);
        // `for x in [1, 2]` style: a collection nothing else decides is an array.
        if let Some(VarKind::Collection { element, len }) = self.u.var_kind(found) {
            let array = self.u.intern(Ty::Array { element, len });
            let _ = self.u.unify(found, array);
        }
        // Through a reference, what it points to is matched, and bindings borrow from it.
        let (matched, by_ref) = match *self.u.get(self.u.shallow(found)) {
            Ty::Ref { mutable, to } => (to, Some(mutable)),
            _ => (found, None),
        };

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
                self.diverging.insert(arm.body);
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
                    } else if let Ty::Optional(inner) = *self.u.get(self.u.shallow(ty))
                        && self.u.unify(inner, expected).is_ok()
                    {
                        // `none` after values: they're all optionals, the values wrapped.
                        self.wrapped.extend(valued.iter().copied());
                        result = Some((ty, first));
                    } else {
                        let from = self.span(first);
                        self.mismatch(expected, ty, self.span(arm.body), Some(from));
                    }
                }
            }
            valued.push(arm.body);
        }

        if rows.iter().all(Option::is_some) && *self.u.get(self.u.shallow(matched)) != Ty::Error
        {
            let pats: Vec<Pat> = rows.into_iter().flatten().collect();
            self.check_coverage(id, arms, &pats, matched);
        }

        match result {
            _ if discarded => self.u.table.unit,
            Some((ty, _)) => ty,
            // Every arm leaves: there's never a value, so it fits wherever it's used.
            None if !arms.is_empty() => self.u.fresh(VarKind::Any),
            None => self.u.table.unit,
        }
    }

    /// Reports arms that can never be reached, and values no arm matches.
    fn check_coverage(&mut self, id: Id<Expr>, arms: &[Arm], pats: &[Pat], matched: TypeId) {
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
            // A few of the values no arm matches, to name.
            let mut missing = Vec::new();
            while missing.len() < 8
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

    /// A fix adding an arm for each of `missing` at the end of the `match` `id`, with an
    /// empty body to fill in.
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

    /// The ways of making a value of `ty`, for `useful`.
    fn ctors(&mut self, ty: TypeId) -> Option<Vec<(Ctor, Vec<TypeId>)>> {
        match self.u.get(self.u.shallow(ty)).clone() {
            Ty::Bool => Some(vec![(Ctor::Bool(false), vec![]), (Ctor::Bool(true), vec![])]),
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

    /// A pattern for the values `pat` stands for, of type `ty`, as it would be written.
    fn render(&mut self, pat: &Pat, ty: TypeId) -> String {
        match pat {
            Pat::Wild => "_".into(),
            Pat::Ctor(Ctor::Bool(b), _) => b.to_string(),
            Pat::Ctor(Ctor::Int(n), _) => n.to_string(),
            Pat::Ctor(Ctor::None, _) => "none".into(),
            // A pattern for an optional's value is written as one for the value.
            Pat::Ctor(Ctor::Some, inner) => match *self.u.get(self.u.shallow(ty)) {
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

    // --- Patterns ---

    /// Checks pattern `id` against a value of type `ty`, giving its bindings their types.
    /// `by_ref`: the value is matched through a reference, `&mut` or not, so bindings borrow
    /// from it. `none_matched`: an earlier arm matched `none`, so a binding of an optional
    /// binds the value it holds.
    fn check_pattern(
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
        let optional = match *self.u.get(shallow) {
            Ty::Optional(inner) => Some(inner),
            _ => None,
        };

        let pat = match &ast.patterns[id] {
            Pattern::Wildcard => Pat::Wild,
            // A name that's a variant, `Empty`; or looks like one, against an enum, where a new
            // name would quietly match everything: `Emtpy`.
            Pattern::Binding { name, .. }
                if self.names.pattern(id).is_none()
                    || self.variant_index(optional.unwrap_or(ty), &name.name).is_some()
                    || (name.name.starts_with(|c: char| c.is_ascii_uppercase())
                        && self.enum_of(optional.unwrap_or(ty)).is_some()) =>
            {
                // A name that's a variant: `Empty`.
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
            Pattern::Variant { path, fields } => {
                self.unwrapped(ty, optional, &mut info, |c, ty, info| {
                    c.variant_pattern(id, ty, path.name(), fields, by_ref, info)
                })
            }
        };
        let _ = span;
        self.patterns.insert(id, info);
        pat
    }

    /// Runs `check` on the type matched: the value an optional holds, if `optional` is the
    /// type that holds, since a pattern other than `none` and a binding matches that value.
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

    /// The index of `ty`'s variant called `name`, if `ty` is an enum and has one.
    fn variant_index(&self, ty: TypeId, name: &str) -> Option<usize> {
        let (s, _) = self.enum_of(ty)?;
        (self.ast.structs[s].variants().iter()).position(|v| &*v.name.name == name)
    }

    /// `Circle(r)`, `Rect { w, .. }` or `Empty` against a value of type `ty`.
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
            (PatternFields::Named { fields: given, rest }, VariantKind::Named) => {
                let mut seen = vec![false; field_tys.len()];
                for (field, pat) in given {
                    match variant.fields.iter().position(|f| f.name.name == field.name) {
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
    fn bind_all(&mut self, id: Id<Pattern>, ty: TypeId) {
        self.patterns.entry(id).or_default();
        match &self.ast.patterns[id] {
            Pattern::Binding { .. } => {
                if let Some(def) = self.names.pattern(id)
                    && matches!(self.names.def(def).kind, DefKind::Binding(_))
                {
                    self.locals.insert(def, ty);
                }
            }
            Pattern::Variant { fields, .. } => self.bind_fields(fields, ty),
            _ => {}
        }
    }
}

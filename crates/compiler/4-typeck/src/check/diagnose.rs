use som_ast::{BinOp, Expr, Func, Id, Span};
use som_base::{Fix, Message, message, msg};
use som_lang::LangTrait;
use som_resolve::DefKind;

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId, Value};
use crate::unify::VarKind;
use crate::{Loop, Take};

impl Checker<'_> {
    /// A type for an error message. A type that isn't known yet can't be written down, so
    /// it's described in plain words instead: "an integer", not a type called that.
    pub(super) fn describe(&self, ty: TypeId) -> Message {
        let text = match self.u.var_kind(ty) {
            Some(VarKind::Int) => "an integer".to_string(),
            Some(VarKind::Float) => "a float".to_string(),
            Some(VarKind::Text) => "a string".to_string(),
            Some(VarKind::Collection { len, .. })
                if *self.u.shallow_ty(len) == Ty::Value(Value::Int(1)) =>
            {
                "a collection of 1 element".to_string()
            }
            Some(VarKind::Collection { len, .. }) => {
                format!(
                    "a collection of {} elements",
                    self.u.display(len, self.names)
                )
            }
            Some(VarKind::Any) => "an unknown type".to_string(),
            None => return message::ty(self.u.display(ty, self.names)).into(),
        };
        text.into()
    }

    pub(super) fn mismatch(
        &mut self,
        expected: TypeId,
        found: TypeId,
        span: Span,
        expected_from: Option<Span>,
    ) {
        let optional = self
            .unexpected_optional(expected, found)
            .then(|| (span.get_text().to_string(), false));
        self.errors.push(TypeError::Mismatch {
            expected: self.describe(expected),
            found: self.describe(found),
            span,
            expected_from,
            fix: None,
            optional,
        });
    }

    /// Like `mismatch`, for the value of `expr`: a `&T` where a `T` is expected gets a fix
    /// dereferencing it.
    pub(super) fn mismatch_at(
        &mut self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
        expected_from: Option<Span>,
    ) {
        if let Some(error) = self.not_borrowed(expr, expected, found, expected_from.clone()) {
            self.errors.push(error);
            return;
        }
        // `fflush(none)`: a null pointer is `mem::null()`; `none` is an optional's.
        if matches!(self.ast.exprs[expr], Expr::None)
            && matches!(self.u.shallow_ty(expected), Ty::Pointer | Ty::Ptr(_))
        {
            self.errors.push(TypeError::NoneAsPointer {
                ty: self.describe(expected),
                span: self.span(expr),
                expected_from,
            });
            // Reported: not also that its type can't be inferred.
            self.exprs[expr.id] = Some(self.u.table.error);
            return;
        }
        let fix = (self.deref_fix(expr, expected, found))
            .or_else(|| self.fallback_fix(expr, expected, found));
        let span = self.span(expr);
        let optional = self.unexpected_optional(expected, found).then(|| {
            let variable = matches!(self.ast.exprs[expr], Expr::Var(_));
            (span.get_text().to_string(), variable)
        });
        self.errors.push(TypeError::Mismatch {
            expected: self.describe(expected),
            found: self.describe(found),
            span,
            expected_from,
            fix,
            optional,
        });
    }

    /// An optional `expr` of type `found` where what it holds, `expected`, is: a fallback for
    /// `none`, where there's an obvious one. `x else 0`, or for a reference to a copy,
    /// `*(nums.get(0) else &0)`.
    fn fallback_fix(&self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> Option<Fix> {
        let Ty::Optional(inner) = *self.u.shallow_ty(found) else {
            return None;
        };
        let literal = |ty: TypeId| {
            let ty = self.u.shallow(ty);
            match (self.u.get(ty), self.u.var_kind(ty)) {
                (_, Some(VarKind::Int)) | (Ty::Int(_), _) => Some("0"),
                (_, Some(VarKind::Float)) | (Ty::Float(_), _) => Some("0.0"),
                (Ty::Bool, _) => Some("false"),
                _ => None,
            }
        };
        let span = self.span(expr);
        let text = span.get_text();
        if text.contains('\n') {
            return None;
        }
        let replacement = if self.u.could_fit(expected, inner) {
            let fallback = format!("{text} else {}", literal(inner)?);
            // `else` binds loosest: parentheses unless it stands alone, as `let`'s value or
            // an argument.
            if stands_alone(&span) {
                fallback
            } else {
                format!("({fallback})")
            }
        } else if let Ty::Ref { to, .. } = *self.u.shallow_ty(inner)
            && self.u.could_fit(expected, to)
            && self.is_copy(to)
        {
            format!("*({text} else &{})", literal(to)?)
        } else {
            return None;
        };
        Some(Fix::replace(
            msg![
                "give a fallback for ",
                message::code("none"),
                ": ",
                message::code(&replacement)
            ],
            span,
            replacement,
        ))
    }

    /// An optional, `found`, where `expected` isn't one: what it holds has to be got at.
    fn unexpected_optional(&self, expected: TypeId, found: TypeId) -> bool {
        let is_optional = |ty| matches!(self.u.shallow_ty(ty), Ty::Optional(_));
        is_optional(found) && !is_optional(expected)
    }

    /// `expr`, a value, where a reference to one is expected: `Hash::hash(key, …)` for
    /// `&self`. The reference type is written from the value's, `&K`, as the call may not
    /// have worked out what it refers to; and the fix borrows it, `&key` or `&mut key`.
    fn not_borrowed(
        &self,
        expr: Id<Expr>,
        expected: TypeId,
        found: TypeId,
        expected_from: Option<Span>,
    ) -> Option<TypeError> {
        // A reference, or an optional one, `&V?`, which a reference is wrapped into.
        let shallow = self.u.shallow(expected);
        let (reference, optional) = match *self.u.get(shallow) {
            Ty::Optional(inner) => (self.u.shallow(inner), true),
            _ => (shallow, false),
        };
        let Ty::Ref { mutable, to } = *self.u.get(reference) else {
            return None;
        };
        if matches!(
            self.u.shallow_ty(found),
            Ty::Ref { .. } | Ty::Optional(_) | Ty::Error
        ) || !self.u.could_fit(to, found)
        {
            return None;
        }
        let borrow = if mutable { "&mut " } else { "&" };
        let found_text = self.u.display(found, self.names);
        let question = if optional { "?" } else { "" };
        let span = self.span(expr);
        // `&a.b`, `&f()` and `&0` borrow as they are; anything looser, in parentheses.
        let edits = if self.binds_tightly(expr) {
            vec![(span.before(), borrow.to_string())]
        } else {
            vec![
                (span.before(), format!("{borrow}(")),
                (span.after(), ")".to_string()),
            ]
        };
        Some(TypeError::NotBorrowed {
            expected: message::code(format!("{borrow}{found_text}{question}")).into(),
            field: matches!(self.ast.exprs[expr], Expr::Field { .. }),
            found: self.describe(found),
            span: span.clone(),
            expected_from,
            fix: Fix {
                title: msg![
                    "borrow it: ",
                    message::code(format!("{borrow}{}", span.get_text()))
                ],
                edits,
            },
        })
    }

    /// A fix for `expr`, a reference to what's expected. A value that copies is dereferenced,
    /// `*expr`. A type parameter is required to copy, `T: Copy`: for a loop's variable that's
    /// all it takes, as the loop then copies its values, and anything else is dereferenced
    /// too. A value that moves can't be taken out of a reference at all: no fix.
    /// Whether `expr` can take a prefix, `&` or `*`, without parentheses: a name, what's
    /// read from one, a call, a literal.
    fn binds_tightly(&self, expr: Id<Expr>) -> bool {
        matches!(
            self.ast.exprs[expr],
            Expr::Var(_)
                | Expr::Field { .. }
                | Expr::Index { .. }
                | Expr::Call { .. }
                | Expr::MethodCall { .. }
                | Expr::Deref(_)
                | Expr::Int(_)
                | Expr::Float(_)
                | Expr::Bool(_)
                | Expr::Str(_)
                | Expr::Array(_)
        )
    }

    fn deref_fix(&self, expr: Id<Expr>, expected: TypeId, found: TypeId) -> Option<Fix> {
        let Ty::Ref { to, .. } = *self.u.shallow_ty(found) else {
            return None;
        };
        if !self.u.could_fit(expected, to) || self.u.could_fit(expected, found) {
            return None;
        }
        let param = matches!(self.u.shallow_ty(to), Ty::Param(_));
        if !self.is_copy(to) && !param {
            return None;
        }
        let span = self.span(expr);
        // A name, or what's read from one, takes a `*` as it is; anything else in parentheses.
        let deref = if self.binds_tightly(expr) {
            vec![(span.before(), "*".to_string())]
        } else {
            vec![
                (span.before(), "*(".to_string()),
                (span.after(), ")".to_string()),
            ]
        };
        let deref_title = msg![
            "dereference it: ",
            message::code(format!("*{}", span.get_text()))
        ];
        if !param || self.is_copy(to) {
            return Some(Fix {
                title: deref_title,
                edits: deref,
            });
        }
        let bound = self.operator_fix(to, LangTrait::Copy, None)?;
        if self.lent_loop_variable(expr) {
            return Some(bound);
        }
        let mut edits = bound.edits;
        edits.extend(deref);
        Some(Fix {
            title: msg![bound.title, ", and ", deref_title],
            edits,
        })
    }

    /// Whether `expr` is the variable of a `for` loop that lends its values because they may
    /// move: one that would copy them if they're required to copy.
    fn lent_loop_variable(&self, expr: Id<Expr>) -> bool {
        let Expr::Var(_) = self.ast.exprs[expr] else {
            return false;
        };
        let Some(def) = self.names.expr(expr) else {
            return false;
        };
        let DefKind::Local(stmt) = self.names.def(def).kind else {
            return false;
        };
        matches!(
            self.records.loops.get(&stmt),
            Some(Loop::Elements { take: Take::Ref })
                | Some(Loop::Iterator {
                    start: crate::Start::Iter,
                    copied: false
                })
        )
    }

    pub(super) fn invalid_operands(&mut self, expr: Id<Expr>, op: BinOp, lhs: TypeId, rhs: TypeId) {
        // An optional operand is the usual reason: say how to get at its value.
        let optional = match self.ast.exprs[expr] {
            Expr::Binary {
                lhs: lhs_expr,
                rhs: rhs_expr,
                ..
            } => [(lhs_expr, lhs), (rhs_expr, rhs)]
                .into_iter()
                .find_map(|(e, ty)| self.optional_operand(e, ty)),
            _ => None,
        };
        // A struct's operators are its traits': say which to implement, or for a type
        // parameter, to require.
        let implement = op.lang_trait().and_then(|lang| {
            let ty = self.u.shallow_ty(lhs);
            match ty {
                Ty::Param(def) => Some((lang.name(), Some(self.names.def(*def).name.to_string()))),
                Ty::Var(_) | Ty::Error | Ty::Optional(_) => None,
                _ if self.has_builtin(lhs, lang) => None,
                // `string`'s operators are the standard library's to give.
                Ty::Struct(def, _) if self.in_std(*def) => None,
                _ => Some((lang.name(), None)),
            }
        });
        let fix = op
            .lang_trait()
            .filter(|_| implement.is_some())
            .and_then(|lang| self.operator_fix(lhs, lang, Some(rhs)))
            .or_else(|| self.interpolation_fix(expr, op, lhs, rhs));
        let takes = op
            .lang_trait()
            .map(|lang| self.operator_takes(lhs, lang))
            .unwrap_or_default();
        self.errors.push(TypeError::InvalidOperands {
            op: op.symbol().into(),
            lhs: self.describe(lhs),
            rhs: self.describe(rhs),
            span: self.ast.exprs.span(expr).clone(),
            optional,
            implement,
            fix,
            takes,
        });
    }

    /// `"score: " + n`, text and something that isn't: written into the text instead,
    /// `"score: {n}"`, as `+` only joins text.
    fn interpolation_fix(
        &self,
        expr: Id<Expr>,
        op: BinOp,
        lhs: TypeId,
        rhs: TypeId,
    ) -> Option<Fix> {
        let Expr::Binary {
            lhs: left,
            rhs: right,
            ..
        } = self.ast.exprs[expr]
        else {
            return None;
        };
        let text = self.u.table.text;
        let is_text = |c: &Self, ty: TypeId| {
            c.u.shallow(ty) == text || c.u.var_kind(ty) == Some(VarKind::Text)
        };
        if op != BinOp::Add || is_text(self, lhs) == is_text(self, rhs) {
            return None;
        }
        // Each side: a literal's text as it is, anything else as a value written in.
        let side = |c: &Self, e: Id<Expr>| -> Option<String> {
            let written = c.span(e).get_text();
            match c.ast.exprs[e] {
                Expr::Str(_) => Some(written[1..written.len() - 1].to_string()),
                _ if written.contains(['"', '\n', '{', '}']) => None,
                _ => Some(format!("{{{written}}}")),
            }
        };
        let joined = format!("\"{}{}\"", side(self, left)?, side(self, right)?);
        Some(Fix::replace(
            msg!["write it into the text: ", message::code(joined.clone())],
            self.span(expr),
            joined,
        ))
    }

    /// For an operand `expr` of optional type `ty`: how it's written, a fallback value to
    /// suggest for it, and where it is.
    fn optional_operand(&self, expr: Id<Expr>, ty: TypeId) -> Option<(String, String, Span)> {
        let Ty::Optional(inner) = *self.u.shallow_ty(ty) else {
            return None;
        };
        let text = self.span(expr).get_text().to_string();
        if text.len() > 30 || text.contains('\n') {
            return None;
        }
        let fallback = match (self.u.shallow_ty(inner), self.u.var_kind(inner)) {
            (_, Some(VarKind::Int)) | (Ty::Int(_), _) => "0",
            (_, Some(VarKind::Float)) | (Ty::Float(_), _) => "0.0",
            _ => "…",
        };
        Some((text, fallback.to_string(), self.span(expr)))
    }

    /// `name<T>(a: i32, b: T) -> T`, for messages.
    pub(super) fn signature_text(&self, func: Id<Func>) -> String {
        let f = &self.ast.funcs[func];
        let generics: Vec<&str> = f.generics.iter().map(|g| &*g.name.name).collect();
        let generics = match generics.is_empty() {
            true => String::new(),
            false => format!("<{}>", generics.join(", ")),
        };
        let params: Vec<String> = (f.params.iter().zip(&self.sigs[func.id].params))
            .map(|(p, &t)| format!("{}: {}", p.name, self.u.display(t, self.names)))
            .collect();
        let ret = self.sigs[func.id].ret;
        let ret = match self.u.shallow_ty(ret) {
            Ty::Unit => String::new(),
            _ => format!(" -> {}", self.u.display(ret, self.names)),
        };
        format!("{}{generics}({}){ret}", f.name, params.join(", "))
    }

    pub(super) fn get_suggestion(&self, object: Id<Expr>, index: Id<Expr>) -> String {
        format!(
            "{}.get({})",
            self.span(object).get_text(),
            self.span(index).get_text()
        )
    }
}

/// Whether the code at `span` stands alone between its neighbours: a `let`'s value, an
/// argument, what's returned. Anything put in its place needs no parentheses there.
pub(super) fn stands_alone(span: &Span) -> bool {
    let source = span.source.content();
    let before = source[..span.start_offset].trim_end();
    let after = source[(span.start_offset + span.length).min(source.len())..].trim_start();
    let opens = match before.chars().last() {
        Some('(' | ',' | '{') => true,
        // `=`, but not `==`, `!=`, `<=` or `>=`.
        Some('=') => !before[..before.len() - 1].ends_with(['=', '!', '<', '>']),
        _ => before.ends_with("return"),
    };
    opens && after.starts_with([';', ',', ')', '}'])
}

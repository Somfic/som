use som_ast::{Expr, Id, Span};
use som_base::{Fix, message, msg};
use som_resolve::visibility::{self, Hidden, Viewer};
use som_resolve::{DefId, DefKind};

use super::Checker;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId};

impl Checker<'_> {
    pub(super) fn viewer<'s>(&self, at: &'s Span) -> Viewer<'s> {
        let file = at.source.identifier();
        let module = (self.ast.modules.iter())
            .position(|m| m.files.iter().any(|f| f.identifier() == file))
            .unwrap_or(usize::MAX);
        let owner = (self.current_func)
            .and_then(|f| self.names.impl_of(f))
            .and_then(|i| self.names.impls[i].owner);
        Viewer {
            module,
            file,
            owner,
        }
    }

    fn report_hidden(&mut self, name: &str, span: &Span, hidden: Option<Hidden>) {
        if let Some(hidden) = hidden {
            self.errors.push(TypeError::NotVisible {
                name: name.to_string(),
                span: span.clone(),
                hidden,
            });
        }
    }

    pub(super) fn check_visible(&mut self, def: DefId, span: &Span) {
        let hidden = visibility::hidden(self.ast, self.names, def, self.viewer(span));
        self.report_hidden(&self.names.def(def).name.clone(), span, hidden);
    }

    /// Functions without `self` returning struct `def` that code at `span` can call, as fixes
    /// replacing `Hello { … }` with a call: arguments from `given` fields of the same name, or
    /// else the parameter names.
    pub(super) fn makers(
        &self,
        def: DefId,
        path: &str,
        given: &[(som_ast::Ident, Id<Expr>)],
        span: &Span,
    ) -> Vec<Fix> {
        let viewer = self.viewer(span);
        let mut fixes = Vec::new();
        for (name, method) in self.names.methods_of(def) {
            let DefKind::Func(f) = self.names.def(method).kind else {
                continue;
            };
            let params = &self.ast.funcs[f].params;
            if params
                .first()
                .is_some_and(|p| &*p.name.name == som_lang::SELF_VALUE)
                || !visibility::can_see(self.ast, self.names, method, viewer)
            {
                continue;
            }
            // A `Self`, or a `Self?`: `File::open` makes one if it can.
            let mut ret = self.u.shallow(self.sigs[f.id].ret);
            if let Ty::Optional(inner) = *self.u.get(ret) {
                ret = self.u.shallow(inner);
            }
            if !matches!(self.u.get(ret), Ty::Struct(d, _) if *d == def) {
                continue;
            }
            let args: Vec<String> = (params.iter())
                .map(|p| {
                    (given.iter())
                        .find(|(field, _)| field.name == p.name.name)
                        .map_or(p.name.name.to_string(), |(_, value)| {
                            self.span(*value).get_text().to_string()
                        })
                })
                .collect();
            let call = format!("{path}::{name}");
            fixes.push(Fix {
                title: msg!["call ", message::code(call.clone())],
                edits: vec![(span.clone(), format!("{call}({})", args.join(", ")))],
            });
        }
        fixes
    }

    /// Where a property stands in for a field of the same name that can't be seen: the type's
    /// name, and why the field is hidden.
    pub(super) fn hidden_field_named(
        &self,
        object_ty: TypeId,
        field: &som_ast::Ident,
    ) -> Option<(String, Hidden)> {
        let mut ty = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let Ty::Struct(def, _) = self.u.get(ty) else {
            return None;
        };
        let DefKind::Struct(s) = self.names.def(*def).kind else {
            return None;
        };
        let strukt = &self.ast.structs[s];
        let i = (!strukt.is_enum())
            .then(|| strukt.fields.iter().position(|f| f.name.name == field.name))??;
        let hidden =
            visibility::hidden_field(self.ast, self.names, s, i, self.viewer(&field.span))?;
        Some((self.names.def(*def).name.to_string(), hidden))
    }

    pub(super) fn field_seen(&self, object_ty: TypeId, field: &som_ast::Ident) -> bool {
        let mut ty = self.u.shallow(object_ty);
        if let Ty::Ref { to, .. } = self.u.get(ty) {
            ty = self.u.shallow(*to);
        }
        let Ty::Struct(def, _) = self.u.get(ty) else {
            return false;
        };
        let DefKind::Struct(s) = self.names.def(*def).kind else {
            return false;
        };
        let strukt = &self.ast.structs[s];
        if strukt.is_enum() {
            return false;
        }
        (strukt.fields.iter())
            .position(|f| f.name.name == field.name)
            .is_some_and(|i| {
                let viewer = self.viewer(&field.span);
                visibility::hidden_field(self.ast, self.names, s, i, viewer).is_none()
            })
    }

    pub(super) fn check_field_visible(
        &mut self,
        s: Id<som_ast::Struct>,
        index: usize,
        span: &Span,
    ) {
        let hidden = visibility::hidden_field(self.ast, self.names, s, index, self.viewer(span));
        let name = self.ast.structs[s].fields[index].name.name.clone();
        self.report_hidden(&name, span, hidden);
    }
}

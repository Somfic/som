use som_ast::{Expr, Id, TypeExpr, VariantKind};
use som_resolve::DefKind;
use som_resolve::visibility::{self};

use super::Checker;
use super::matching::holds;
use crate::error::TypeError;
use crate::ty::{Ty, TypeId, Value};
use crate::unify::VarKind;

impl Checker<'_> {
    pub(super) fn infer_array(&mut self, items: &[Id<Expr>]) -> TypeId {
        let element = self.u.fresh(VarKind::Any);
        let found: Vec<TypeId> = items.iter().map(|&item| self.infer(item)).collect();
        // `[1, none, 5]`: with an optional among them, they're all optionals, and the
        // plain values are wrapped, as in `x if c else none`.
        if let Some(&optional) = found
            .iter()
            .find(|&&ty| matches!(self.u.get(self.u.shallow(ty)), Ty::Optional(_)))
        {
            let _ = self.u.unify(element, optional);
        }
        for (&item, &ty) in items.iter().zip(&found) {
            if !self.coerce(item, element, ty) {
                self.mismatch(element, ty, self.span(item), None);
            }
        }
        let len = self.u.intern(Ty::Value(Value::Int(items.len() as u128)));
        self.u.fresh(VarKind::Collection { element, len })
    }

    pub(super) fn infer_construct(
        &mut self,
        id: Id<Expr>,
        path: &som_ast::Path,
        written: &[Id<TypeExpr>],
        given: &[(som_ast::Ident, Id<Expr>)],
    ) -> TypeId {
        let error = self.u.table.error;
        let Some(def) = self.names.expr(id) else {
            given.iter().for_each(|(_, v)| {
                self.infer(*v);
            });
            return error;
        };
        // A struct, or a variant with named fields: `Shape::Rect { w: 1.0, h: 2.0 }`.
        let (s, variant) = match self.names.def(def).kind {
            DefKind::Struct(s) => (s, None),
            DefKind::Variant { strukt, index } => (strukt, Some(index)),
            _ => unreachable!("resolver only binds constructors to structs and variants"),
        };
        let def = self.names.strukt(s);
        // The type arguments written, `Pair<i32> { … }`; or each starts unknown, and the
        // fields' values decide them.
        let expected = self.names.struct_params(s).len();
        // `Self { … }` is the type the impl block is for, arguments and all: in
        // `impl Vector<3, f32>`, a `Vector<3, f32>`.
        let of_self = (path.segments.len() == 1 && &*path.segments[0].name == som_lang::SELF_TYPE)
            .then(|| self.current_func.and_then(|f| self.names.impl_of(f)))
            .flatten()
            .map(|index| self.self_type(index))
            // `Vector3 { … }`: the type the alias stands for.
            .or_else(|| {
                self.names
                    .expr_alias(id)
                    .map(|alias| self.alias_value(alias))
            })
            .and_then(|t| match self.u.get(self.u.shallow(t)) {
                Ty::Struct(d, args) if *d == def => Some(args.clone()),
                _ => None,
            });
        let args: Vec<TypeId> = if let Some(args) = of_self {
            args
        } else if written.is_empty() {
            (0..expected).map(|_| self.u.fresh(VarKind::Any)).collect()
        } else {
            self.lower_args(def, s, written, self.span(id))
        };
        let subst = self.struct_subst(s, &args);
        let ty = self.u.intern(Ty::Struct(def, args.clone()));
        let ast_struct = &self.ast.structs[s];
        let wrong_form = match variant {
            // `Shape { … }`: an enum is made from one of its variants.
            None if ast_struct.is_enum() => {
                let example = (!ast_struct.variants().is_empty()).then(|| self.variant_form(s, 0));
                self.errors.push(TypeError::ConstructEnum {
                    name: ast_struct.name.name.to_string(),
                    example,
                    span: self.span(id),
                });
                true
            }
            Some(index) if ast_struct.variants()[index].kind != VariantKind::Named => {
                self.errors.push(TypeError::WrongVariantForm {
                    name: self.variant_name(s, index),
                    holds: holds(ast_struct.variants()[index].kind),
                    form: self.variant_form(s, index),
                    span: self.span(id),
                });
                true
            }
            _ => false,
        };
        if wrong_form {
            given.iter().for_each(|(_, v)| {
                self.infer(*v);
            });
            return ty;
        }
        let (struct_name, declared, field_tys) = match variant {
            Some(index) => (
                self.variant_name(s, index),
                &ast_struct.variants()[index].fields,
                self.variant_fields(s, index, &args),
            ),
            None => {
                let tys = (0..ast_struct.fields.len())
                    .map(|i| self.substitute(self.structs[s.id][i], &subst))
                    .collect();
                (
                    self.names.def(def).name.to_string(),
                    &ast_struct.fields,
                    tys,
                )
            }
        };
        // Only C can make one: som doesn't know what's inside.
        if self.ast.structs[s].handle {
            self.errors.push(TypeError::ConstructHandle {
                name: struct_name,
                span: self.span(id),
            });
            given.iter().for_each(|(_, v)| {
                self.infer(*v);
            });
            return ty;
        }
        // Fields this code can't see: it can't set them, so it can't make one.
        let at = self.span(id);
        let hidden: Vec<usize> = match variant {
            Some(_) => Vec::new(),
            None => (0..declared.len())
                .filter(|&i| {
                    let viewer = self.viewer(&at);
                    visibility::hidden_field(self.ast, self.names, s, i, viewer).is_some()
                })
                .collect(),
        };
        let mut seen = vec![false; declared.len()];
        for (name, value) in given {
            match declared.iter().position(|f| f.name.name == name.name) {
                Some(index) if seen[index] => {
                    self.errors.push(TypeError::DuplicateField {
                        field: name.name.to_string(),
                        span: name.span.clone(),
                    });
                    self.infer(*value);
                }
                Some(index) => {
                    seen[index] = true;
                    let expected = field_tys[index];
                    let from = self.ast.types.span(declared[index].ty).clone();
                    self.expect(*value, expected, Some(from));
                }
                None => {
                    self.errors.push(TypeError::UnknownField {
                        ty: struct_name.clone(),
                        field: name.name.to_string(),
                        available: declared.iter().map(|f| f.name.name.to_string()).collect(),
                        props: Vec::new(),
                        methods: Vec::new(),
                        span: name.span.clone(),
                    });
                    self.infer(*value);
                }
            }
        }
        if !hidden.is_empty() {
            self.errors.push(TypeError::ConstructHidden {
                ty: struct_name,
                fields: (hidden.iter())
                    .map(|&i| {
                        let name = &declared[i].name;
                        (name.name.to_string(), name.span.clone())
                    })
                    .collect(),
                span: path.span(),
                fixes: self.makers(def, &path.to_string(), given, &at),
            });
            return ty;
        }
        for (field, _) in declared.iter().zip(&seen).filter(|(_, seen)| !**seen) {
            self.errors.push(TypeError::MissingField {
                ty: struct_name.clone(),
                field: field.name.name.to_string(),
                span: self.span(id),
            });
        }
        ty
    }
}

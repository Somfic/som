use std::collections::HashSet;

use som_ast::{Expr, Id, Struct};
use som_resolve::visibility;
use som_resolve::{DefId, DefKind};
use som_typeck::{ArrayMethod, PointerMethod, Ty, TypeId};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use super::Request;
use super::items::{method_item, prop_items, snippet_item, takes_self, trait_methods_of};
use crate::docs::{
    Sequence, array_method_doc, array_of, describe, doc_of, markdown, pointer_method_doc,
    readable_fields, signature, signature_with, ty,
};
use crate::lookup::receiver_before;
use crate::text_scan::dot_before_word;
use crate::types::{method_subst, struct_of, struct_subst, unref};

/// After a dot, as in `alloc(10).` or `alloc(10).si`: what the value before it has.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let dot = dot_before_word(request.text, request.offset)?;
    let a = request.a;
    let Some(receiver) = receiver_before(a, request.file, request.text, dot) else {
        return Some(Vec::new());
    };
    let t = a.typed.expr(receiver);
    Some(if let Some(sequence) = array_of(a, t) {
        array_methods(request, sequence)
    } else if let Ty::Ptr(element) = *a.typed.ty(t) {
        pointer_methods(request, element)
    } else if let Some(owner) = struct_of(a, t)
        && let DefKind::Struct(s) = a.names.def(owner).kind
    {
        struct_members(request, receiver, owner, s)
    } else {
        other_members(request, t)
    })
}

fn array_methods(request: &Request, sequence: Sequence) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for &method in ArrayMethod::ALL {
        // Text can only be read, and only text has `bytes`.
        let skip = if sequence.is_text {
            matches!(
                method,
                ArrayMethod::GetMut | ArrayMethod::Set | ArrayMethod::SliceMut
            )
        } else {
            method == ArrayMethod::Bytes
        };
        if skip {
            continue;
        }
        let doc = array_method_doc(request.a, method, sequence);
        let kind = if method == ArrayMethod::Len {
            CompletionItemKind::PROPERTY
        } else {
            CompletionItemKind::METHOD
        };
        items.push(CompletionItem {
            label: doc.name.to_string(),
            kind: Some(kind),
            detail: Some(doc.signature),
            documentation: markdown(Some(doc.doc)),
            ..Default::default()
        });
    }
    items
}

fn pointer_methods(request: &Request, element: TypeId) -> Vec<CompletionItem> {
    let element = ty(request.a, element);
    (PointerMethod::ALL.iter())
        .map(|&method| {
            let doc = pointer_method_doc(method, &element);
            let snippet = (!request.insert.call_follows).then(|| {
                let args: Vec<String> = (doc.param_names.iter().enumerate())
                    .map(|(i, p)| format!("${{{}:{p}}}", i + 1))
                    .collect();
                format!("{}({})$0", doc.name, args.join(", "))
            });
            snippet_item(
                doc.name.to_string(),
                CompletionItemKind::METHOD,
                snippet,
                doc.signature,
                markdown(Some(doc.doc)),
            )
        })
        .collect()
}

/// A struct's fields, props and methods, with a generic one's types as they are here: `i32`,
/// not `T`.
fn struct_members(
    request: &Request,
    receiver: Id<Expr>,
    owner: DefId,
    s: Id<Struct>,
) -> Vec<CompletionItem> {
    let a = request.a;
    let receiver_ty = a.typed.expr(receiver);
    let subst = struct_subst(a, s, receiver_ty);
    let enum_ = a.ast.structs[s].is_enum();
    let mut items = Vec::new();
    let mut fields_here = HashSet::new();
    for (i, (field, t)) in readable_fields(a, s).into_iter().enumerate() {
        // An enum's common fields are as visible as it.
        if !enum_ && visibility::hidden_field(&a.ast, &a.names, s, i, request.viewer).is_some() {
            continue;
        }
        fields_here.insert(field.name.name.to_string());
        items.push(CompletionItem {
            label: field.name.name.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(a.typed.table.display_with(t, &a.names, &subst)),
            documentation: markdown(field.doc.clone()),
            ..Default::default()
        });
    }
    // A prop named like a field seen here isn't what `x.name` reads here.
    let mut props = Vec::new();
    prop_items(a, owner, receiver_ty, request.viewer, &mut props);
    items.extend(
        props
            .into_iter()
            .filter(|p| !fields_here.contains(&p.label)),
    );
    for (name, method) in a.names.methods_of(owner) {
        // Of the overloads called this, one this receiver has: `@if N >= 3 { … }` around
        // some, `else` around others.
        let Some(method) =
            (a.names.overloads(method).into_iter()).find(|&d| match a.names.def(d).kind {
                DefKind::Func(f) => {
                    som_typeck::member_holds(&a.ast, &a.names, &a.typed, f, receiver_ty)
                        != Some(false)
                }
                _ => true,
            })
        else {
            continue;
        };
        // Not those called on the type, `Queue::empty`: a value has only what takes `self`.
        let on_the_type = matches!(a.names.def(method).kind, DefKind::Func(f) if !takes_self(a, f));
        if !request.visible(method) || on_the_type {
            continue;
        }
        let item = match a.names.def(method).kind {
            DefKind::Func(f) => method_item(
                a,
                f,
                name,
                1,
                request.insert,
                signature_with(a, f, &method_subst(a, f, receiver_ty)),
                doc_of(a, method),
            ),
            _ => snippet_item(
                name.to_string(),
                CompletionItemKind::METHOD,
                None,
                describe(a, method),
                markdown(doc_of(a, method)),
            ),
        };
        items.push(item);
    }
    items
}

/// Any other type's own methods, from `impl i32 { … }`, then the methods of the traits it
/// has: `12.compare(&other)`.
fn other_members(request: &Request, t: TypeId) -> Vec<CompletionItem> {
    let a = request.a;
    let mut items = Vec::new();
    let owner = som_typeck::traits::primitive_of(a.typed.ty(unref(a, t)))
        .and_then(|p| a.names.primitive(p));
    if let Some(owner) = owner {
        prop_items(a, owner, t, request.viewer, &mut items);
    }
    for (name, method) in owner.map(|o| a.names.methods_of(o)).unwrap_or_default() {
        let DefKind::Func(f) = a.names.def(method).kind else {
            continue;
        };
        // Not those called on the type, `i32::parse`, nor a trait's, which come with the
        // trait methods below.
        let from_trait = (a.names.impl_of(f)).is_some_and(|i| a.names.impls[i].trait_.is_some());
        if !request.visible(method) || !takes_self(a, f) || from_trait {
            continue;
        }
        items.push(method_item(
            a,
            f,
            name,
            1,
            request.insert,
            signature(a, f),
            doc_of(a, method),
        ));
    }
    for (method, signature) in trait_methods_of(a, t) {
        let name = a.ast.funcs[method].name.name.to_string();
        items.push(method_item(
            a,
            method,
            &name,
            1,
            request.insert,
            signature,
            a.ast.funcs[method].doc.clone(),
        ));
    }
    items
}

use som_ast::{Id, Struct, Trait};
use som_resolve::{DefId, DefKind};
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use super::Request;
use super::items::{
    assoc_const_items, call_snippet, method_item, module_items, snippet_item, variant_snippet,
};
use crate::docs::{describe, doc_of, markdown, signature, variant_text};
use crate::lookup::{find_struct, module_named, modules_in_scope, undeclared_library};
use crate::text_scan::qualifier;

pub(super) enum Answer {
    Items(Vec<CompletionItem>),
    /// What the path names can't be used here.
    Nothing,
    NotQualified,
}

/// After a path and `::`: what's in the module, type or trait it names. `raylib::` lists
/// what's in `raylib`, where its `extern "raylib"` is.
pub(super) fn complete(request: &Request) -> Answer {
    let Some(path) = qualifier(request.text, request.offset) else {
        return Answer::NotQualified;
    };
    let a = request.a;
    let (here, _) = modules_in_scope(a, request.file, request.offset);
    // What isn't a module is named by its last part: `Hash` in `som::traits::Hash`.
    let last = path.rsplit("::").next().unwrap_or(path);
    let items = if let Some(m) = module_named(a, path, request.file, request.offset) {
        if undeclared_library(a, here, m) {
            return Answer::Nothing;
        }
        module_items(a, m, None, request.insert)
    } else if let Some(owner) = a.names.primitives_named(last) {
        primitive_items(request, owner)
    } else if let Some((t, _)) = (a.ast.traits.iter()).find(|(_, t)| *t.name.name == *last) {
        trait_method_items(request, t)
    } else if let Some(s) = find_struct(a, path, request.file, request.offset) {
        struct_items(request, s)
    } else {
        Vec::new()
    };
    Answer::Items(items)
}

/// `i32::` lists its constants, functions and methods.
fn primitive_items(request: &Request, owner: DefId) -> Vec<CompletionItem> {
    let a = request.a;
    let mut items = assoc_const_items(a, owner, &|def| request.visible(def));
    for (name, method) in a.names.methods_of(owner) {
        if !request.visible(method) {
            continue;
        }
        let DefKind::Func(f) = a.names.def(method).kind else {
            continue;
        };
        items.push(method_item(
            a,
            f,
            name,
            0,
            request.insert,
            signature(a, f),
            doc_of(a, method),
        ));
    }
    items
}

/// `Hash::` lists the trait's methods, to call on any type implementing it:
/// `Hash::hash(&value, &mut hasher)`.
fn trait_method_items(request: &Request, t: Id<Trait>) -> Vec<CompletionItem> {
    let a = request.a;
    (a.ast.traits[t].methods.iter())
        .map(|&f| {
            let name = &a.ast.funcs[f].name.name;
            method_item(
                a,
                f,
                name,
                0,
                request.insert,
                signature(a, f),
                doc_of(a, a.names.func(f)),
            )
        })
        .collect()
}

/// `Shape::` lists its variants, then its constants and methods.
fn struct_items(request: &Request, s: Id<Struct>) -> Vec<CompletionItem> {
    let a = request.a;
    let mut items: Vec<CompletionItem> = (a.ast.structs[s].variants().iter().enumerate())
        .map(|(i, variant)| {
            snippet_item(
                variant.name.name.to_string(),
                CompletionItemKind::ENUM_MEMBER,
                variant_snippet(a, s, i, request.insert),
                variant_text(a, s, i),
                markdown(variant.doc.clone()),
            )
        })
        .collect();
    let owner = a.names.strukt(s);
    items.extend(assoc_const_items(a, owner, &|def| request.visible(def)));
    for (name, method) in a.names.methods_of(owner) {
        if !request.visible(method) {
            continue;
        }
        let snippet = match a.names.def(method).kind {
            DefKind::Func(f) => call_snippet(a, f, name, 0, request.insert),
            _ => None,
        };
        items.push(snippet_item(
            name.to_string(),
            CompletionItemKind::METHOD,
            snippet,
            describe(a, method),
            markdown(doc_of(a, method)),
        ));
    }
    items
}

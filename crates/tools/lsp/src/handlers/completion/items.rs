//! Completion items for the things a program declares.

use som_ast::{Func, Id, Item, Struct, Trait};
use som_driver::Analysis;
use som_lang::LangTrait;
use som_resolve::visibility::{self, Viewer};
use som_resolve::{DefId, DefKind};
use som_typeck::{Ty, TypeId};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation, InsertTextFormat,
};

use crate::docs::{describe, doc_of, markdown, signature, ty};
use crate::lookup::aliased_struct;
use crate::text_scan::{is_word, type_position};
use crate::types::{impl_target_matches, method_subst};

/// A struct with more fields than this completes to a literal with empty braces, to fill in
/// with field completion.
const MAX_LITERAL_FIELDS: usize = 6;

/// Where a completion goes, which decides whether it's a snippet.
#[derive(Clone, Copy)]
pub(crate) struct Insert {
    /// A value goes here, not a type: a struct completes to a literal.
    pub value: bool,
    /// The text after the word already opens a call: `init_win|(800, …)`.
    pub call_follows: bool,
}

impl Insert {
    pub(crate) fn at(text: &str, offset: usize) -> Insert {
        let after = text[offset..].trim_start_matches(is_word);
        Insert {
            value: !type_position(text, offset),
            call_follows: after.starts_with('('),
        }
    }
}

/// An item that inserts `snippet` if there is one, or else its label.
pub(super) fn snippet_item(
    label: String,
    kind: CompletionItemKind,
    snippet: Option<String>,
    detail: String,
    documentation: Option<Documentation>,
) -> CompletionItem {
    CompletionItem {
        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
        insert_text: snippet,
        label,
        kind: Some(kind),
        detail: Some(detail),
        documentation,
        ..Default::default()
    }
}

/// A function, completing to a call with a tab stop on each argument:
/// `init_window(${1:width}, ${2:height}, ${3:title})`.
pub(super) fn func_item(
    a: &Analysis,
    f: Id<Func>,
    label: String,
    insert: Insert,
) -> CompletionItem {
    let snippet = call_snippet(a, f, &label, 0, insert);
    snippet_item(
        label,
        CompletionItemKind::FUNCTION,
        snippet,
        signature(a, f),
        markdown(a.ast.funcs[f].doc.clone()),
    )
}

/// What a function reached through its type is: called on a value, `self` first; making one
/// of the type, `Queue::empty`; or neither.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum TypeMember {
    Constructor,
    Function,
    Method,
}

impl TypeMember {
    pub(super) fn of(a: &Analysis, f: Id<Func>, owner: DefId) -> TypeMember {
        let func = &a.ast.funcs[f];
        if (func.params.first()).is_some_and(|p| *p.name.name == *som_lang::SELF_VALUE) {
            return TypeMember::Method;
        }
        match a.typed.ty(a.typed.func(f).ret) {
            Ty::Struct(def, _) if *def == owner => TypeMember::Constructor,
            _ => TypeMember::Function,
        }
    }

    /// Editors that tell them apart show each its own icon; VS Code draws all three alike,
    /// so the label's details say it too.
    fn kind(self) -> CompletionItemKind {
        match self {
            TypeMember::Constructor => CompletionItemKind::CONSTRUCTOR,
            TypeMember::Function => CompletionItemKind::FUNCTION,
            TypeMember::Method => CompletionItemKind::METHOD,
        }
    }

    fn description(self) -> &'static str {
        match self {
            TypeMember::Constructor => "makes one",
            TypeMember::Function => "function",
            TypeMember::Method => "method",
        }
    }
}

/// `Queue::empty`, `Queue::enqueue`: a function of type `owner`, shown as what it is, with its
/// parameters after its name, `self` as it's taken (`&mut self`). After the type's variants
/// and constants: what makes one first, then other functions, then methods.
pub(super) fn type_member_item(
    a: &Analysis,
    f: Id<Func>,
    owner: DefId,
    name: &str,
    insert: Insert,
    doc: Option<String>,
) -> CompletionItem {
    let member = TypeMember::of(a, f, owner);
    let func = &a.ast.funcs[f];
    let params: Vec<String> = (func.params.iter().zip(&a.typed.func(f).params))
        .enumerate()
        .map(|(i, (p, &t))| match a.typed.ty(t) {
            _ if i > 0 || member != TypeMember::Method => p.name.name.to_string(),
            Ty::Ref { mutable: true, .. } => "&mut self".to_string(),
            Ty::Ref { .. } => "&self".to_string(),
            _ => "self".to_string(),
        })
        .collect();
    let mut item = snippet_item(
        name.to_string(),
        member.kind(),
        call_snippet(a, f, name, 0, insert),
        signature(a, f),
        markdown(doc),
    );
    item.label_details = Some(CompletionItemLabelDetails {
        detail: Some(format!("({})", params.join(", "))),
        description: Some(member.description().to_string()),
    });
    // `~` sorts after the variants' and constants' names.
    let order = match member {
        TypeMember::Constructor => 0,
        TypeMember::Function => 1,
        TypeMember::Method => 2,
    };
    item.sort_text = Some(format!("~{order}{name}"));
    item
}

/// A method or function called through its type, `i32::parse`, or on a value, `v.push`:
/// `skip` leaves out the `self` a value gives it.
pub(super) fn method_item(
    a: &Analysis,
    f: Id<Func>,
    name: &str,
    skip: usize,
    insert: Insert,
    detail: String,
    doc: Option<String>,
) -> CompletionItem {
    snippet_item(
        name.to_string(),
        CompletionItemKind::METHOD,
        call_snippet(a, f, name, skip, insert),
        detail,
        markdown(doc),
    )
}

/// `label(${1:a}, ${2:b})$0`: a call of `f` with a tab stop on each argument after the first
/// `skip`. None if a call already follows.
pub(super) fn call_snippet(
    a: &Analysis,
    f: Id<Func>,
    label: &str,
    skip: usize,
    insert: Insert,
) -> Option<String> {
    if insert.call_follows {
        return None;
    }
    let args: Vec<String> = (a.ast.funcs[f].params.iter().skip(skip).enumerate())
        .map(|(i, p)| {
            let out = if p.out { "out " } else { "" };
            format!("{out}${{{}:{}}}", i + 1, p.name.name)
        })
        .collect();
    Some(format!("{label}({})$0", args.join(", ")))
}

/// Variant `index` of enum `s` as a value to fill in: `Circle(${1:f32})`, `Rect { w: $1, h: $2 }`.
pub(super) fn variant_snippet(
    a: &Analysis,
    s: Id<Struct>,
    index: usize,
    insert: Insert,
) -> Option<String> {
    let def = &a.ast.structs[s];
    let variant = &def.variants()[index];
    let start = def.payload_start(index);
    let types = &a.typed.fields(s)[start..start + variant.fields.len()];
    let name = &variant.name.name;
    match variant.kind {
        som_ast::VariantKind::Unit => None,
        som_ast::VariantKind::Tuple if insert.call_follows => None,
        som_ast::VariantKind::Tuple => {
            let args: Vec<String> = (types.iter().enumerate())
                .map(|(i, &t)| format!("${{{}:{}}}", i + 1, ty(a, t)))
                .collect();
            Some(format!("{name}({})$0", args.join(", ")))
        }
        som_ast::VariantKind::Named => {
            let fields: Vec<String> = (variant.fields.iter().enumerate())
                .map(|(i, f)| format!("{}: ${}", f.name, i + 1))
                .collect();
            Some(format!("{name} {{ {} }}$0", fields.join(", ")))
        }
    }
}

pub(super) fn trait_item(a: &Analysis, t: Id<Trait>, label: String) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::INTERFACE),
        detail: Some(format!("trait {}", a.ast.traits[t].name.name)),
        documentation: markdown(a.ast.traits[t].doc.clone()),
        ..Default::default()
    }
}

fn struct_kind(st: &Struct) -> CompletionItemKind {
    match st.is_enum() {
        true => CompletionItemKind::ENUM,
        false => CompletionItemKind::STRUCT,
    }
}

/// A struct or enum by its name, which can go on to `::new()`; and where a value goes, a
/// struct's literal as an item of its own, `Color { r: $1, g: $2, b: $3, a: $4 }`.
pub(super) fn struct_items(
    a: &Analysis,
    s: Id<Struct>,
    label: String,
    insert: Insert,
) -> Vec<CompletionItem> {
    let st = &a.ast.structs[s];
    let mut items = vec![CompletionItem {
        label: label.clone(),
        kind: Some(struct_kind(st)),
        documentation: markdown(st.doc.clone()),
        ..Default::default()
    }];
    if insert.value && !st.is_enum() && !st.handle {
        let snippet = if st.fields.is_empty() {
            format!("{label} {{}}$0")
        } else if st.fields.len() > MAX_LITERAL_FIELDS {
            format!("{label} {{ $0 }}")
        } else {
            let fields: Vec<String> = (st.fields.iter().enumerate())
                .map(|(i, field)| format!("{}: ${}", field.name.name, i + 1))
                .collect();
            format!("{label} {{ {} }}$0", fields.join(", "))
        };
        items.push(CompletionItem {
            label: format!("{label} {{ … }}"),
            filter_text: Some(label.clone()),
            // Right after the name alone.
            sort_text: Some(format!("{label} {{")),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            insert_text: Some(snippet),
            kind: Some(CompletionItemKind::SNIPPET),
            documentation: markdown(st.doc.clone()),
            ..Default::default()
        });
    }
    items
}

/// A struct or enum by its name alone, as a type.
pub(super) fn type_item(a: &Analysis, s: Id<Struct>, label: String) -> CompletionItem {
    let st = &a.ast.structs[s];
    CompletionItem {
        label,
        kind: Some(struct_kind(st)),
        documentation: markdown(st.doc.clone()),
        ..Default::default()
    }
}

/// Type alias `alias`, called `label`. Where a value goes (`insert`), one for a struct also
/// completes to the struct's literal, `Vector3 { values: … }`.
pub(super) fn alias_items(
    a: &Analysis,
    alias: &som_ast::Alias,
    label: String,
    insert: Option<Insert>,
) -> Vec<CompletionItem> {
    let detail = Some(format!(
        "type {} = {}",
        alias.name.name,
        a.ast.type_text(alias.ty)
    ));
    let documentation = markdown(alias.doc.clone());
    match (insert, aliased_struct(a, alias.ty)) {
        (Some(insert), Some(s)) => (struct_items(a, s, label, insert).into_iter())
            .map(|item| CompletionItem {
                detail: detail.clone(),
                documentation: documentation.clone(),
                ..item
            })
            .collect(),
        _ => vec![CompletionItem {
            label,
            kind: Some(CompletionItemKind::STRUCT),
            detail,
            documentation,
            ..Default::default()
        }],
    }
}

/// `Color::RED`: the constants `owner`'s impl blocks give it that `visible` lets be seen.
pub(super) fn assoc_const_items(
    a: &Analysis,
    owner: DefId,
    visible: &dyn Fn(DefId) -> bool,
) -> Vec<CompletionItem> {
    (a.names.assoc_consts_of(owner).into_iter())
        .filter(|&(_, def)| visible(def))
        .map(|(name, def)| const_item(a, def, name.to_string()))
        .collect()
}

fn const_item(a: &Analysis, def: DefId, label: String) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::CONSTANT),
        detail: Some(describe(a, def)),
        documentation: markdown(doc_of(a, def)),
        ..Default::default()
    }
}

/// The functions, types, traits and constants in module `m`, named `module::name` when a
/// module is given.
pub(crate) fn module_items(
    a: &Analysis,
    m: usize,
    module: Option<&str>,
    insert: Insert,
) -> Vec<CompletionItem> {
    let name = |n: &str| match module {
        Some(module) => format!("{module}::{n}"),
        None => n.to_string(),
    };
    let mut funcs: Vec<Id<Func>> = Vec::new();
    let mut items = Vec::new();
    for item in &a.ast.modules[m].items {
        match item {
            Item::Func(id) => funcs.push(*id),
            Item::Extern(block) if block.module.is_none_or(|b| b == m) => {
                funcs.extend(&block.funcs)
            }
            Item::Struct(id) => items.extend(struct_items(
                a,
                *id,
                name(&a.ast.structs[*id].name.name),
                insert,
            )),
            Item::Trait(id) => items.push(trait_item(a, *id, name(&a.ast.traits[*id].name.name))),
            Item::Alias(alias) => {
                items.extend(alias_items(a, alias, name(&alias.name.name), Some(insert)))
            }
            // `@assert(…);` on its own has no name to complete.
            Item::Const(id) if !a.ast.consts[*id].is_named() => {}
            Item::Const(id) => {
                let def = a.names.const_def(*id);
                items.push(const_item(a, def, name(&a.ast.consts[*id].name.name)));
            }
            _ => {}
        }
    }
    // Hand-written `extern "raylib" { … }` blocks elsewhere put their functions here too.
    for (index, other) in a.ast.modules.iter().enumerate() {
        if index == m {
            continue;
        }
        for item in &other.items {
            if let Item::Extern(block) = item
                && block.module == Some(m)
            {
                funcs.extend(&block.funcs);
            }
        }
    }
    for f in funcs {
        let func = &a.ast.funcs[f];
        // Nothing calls `main`, or a `@test`: the runner does.
        if *func.name.name == *som_lang::ENTRY_POINT && module.is_none() && m == 0 || func.test {
            continue;
        }
        items.push(func_item(a, f, name(&func.name.name), insert));
    }
    items
}

/// The properties of `owner` a value of type `receiver` has, as completions: `x.len`. Of
/// several impl blocks with one called the same, only the one for this type, so a
/// `Vector<3, i32>` gets `Vector<3, T>`'s `x`, not `Vector<2, T>`'s.
pub(super) fn prop_items(
    a: &Analysis,
    owner: DefId,
    receiver: TypeId,
    viewer: Viewer,
    items: &mut Vec<CompletionItem>,
) {
    for (name, prop) in a.names.props_of(owner) {
        let DefKind::Func(f) = a.names.def(prop).kind else {
            continue;
        };
        if !visibility::can_see(&a.ast, &a.names, prop, viewer) {
            continue;
        }
        let fits = (a.names.impl_of(f))
            .and_then(|i| a.typed.impls[i].target)
            .is_none_or(|target| impl_target_matches(a, target, receiver));
        // Only there for some instances (`@if N >= 3 { … }`), not this one.
        if !fits || som_typeck::member_holds(&a.ast, &a.names, &a.typed, f, receiver) == Some(false)
        {
            continue;
        }
        let ret = a.typed.func(f).ret;
        let shown = a
            .typed
            .table
            .display_with(ret, &a.names, &method_subst(a, f, receiver));
        items.push(CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(shown),
            documentation: markdown(doc_of(a, prop)),
            ..Default::default()
        });
    }
}

/// The functions of struct `s` that make one, giving a `Self` or a `Self?` and taking no
/// `self`: `File::open`. With each one's name and definition.
pub(crate) fn constructors<'a>(
    a: &'a Analysis,
    s: Id<Struct>,
    visible: &dyn Fn(DefId) -> bool,
) -> Vec<(&'a str, Id<Func>, DefId)> {
    let owner = a.names.strukt(s);
    let makes = |ty: TypeId| match a.typed.ty(ty) {
        Ty::Struct(def, _) => *def == owner,
        Ty::Optional(inner) => matches!(a.typed.ty(*inner), Ty::Struct(def, _) if *def == owner),
        _ => false,
    };
    (a.names.methods_of(owner).into_iter())
        .filter(|&(_, method)| visible(method))
        .filter_map(|(name, method)| match a.names.def(method).kind {
            DefKind::Func(f) => Some((name, f, method)),
            _ => None,
        })
        .filter(|&(_, f, _)| !takes_self(a, f) && makes(a.typed.func(f).ret))
        .collect()
}

pub(super) fn takes_self(a: &Analysis, f: Id<Func>) -> bool {
    (a.ast.funcs[f].params.first()).is_some_and(|p| &*p.name.name == som_lang::SELF_VALUE)
}

/// The methods of the traits values of type `t` have, each with its signature for `t`: the
/// language's traits it has built in (`compare` of a number), then the traits implemented
/// for it.
pub(super) fn trait_methods_of(a: &Analysis, t: TypeId) -> Vec<(Id<Func>, String)> {
    // Each trait with what its parameters are for `t`: `Double<i32>` gives `T` as `i32`.
    let mut traits: Vec<(Id<Trait>, Vec<String>)> = Vec::new();
    let mut add = |def, args: Vec<String>| {
        if let DefKind::Trait(tr) = a.names.def(def).kind
            && !traits.iter().any(|(t, a)| *t == tr && *a == args)
        {
            traits.push((tr, args));
        }
    };
    for &lang in LangTrait::ALL {
        if som_typeck::traits::builtin_operator(a.typed.ty(t), lang)
            && let Some(def) = a.names.lang(lang)
        {
            add(def, Vec::new());
        }
    }
    for (i, info) in a.typed.impls.iter().enumerate() {
        let Some(trait_) = info.trait_ else { continue };
        let this = std::slice::from_ref(&a.typed.impls[i]);
        let Some((_, args)) =
            som_typeck::traits::find_impl(this, &a.typed.table, trait_, t, &[], &|x| x)
        else {
            continue;
        };
        let subst: Vec<_> = info.params.iter().copied().zip(args).collect();
        let shown = (info.trait_args.iter())
            .map(|&arg| a.typed.table.display_with(arg, &a.names, &subst))
            .collect();
        add(trait_, shown);
    }
    let shown = ty(a, t);
    let mut methods = Vec::new();
    for (tr, args) in traits {
        let stubs = som_typeck::stubs::stubs(&a.ast, tr, &shown, &args, &|_, _| false);
        for &method in &a.ast.traits[tr].methods {
            let name = &a.ast.funcs[method].name.name;
            let signature = (stubs.iter())
                .find(|s| !s.is_type && *s.name == **name)
                .map_or_else(|| format!("fn {name}"), |s| s.signature.clone());
            methods.push((method, signature));
        }
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lookup::{find_struct, viewer_at};
    use som_ast::Stmt;

    const VALUE: Insert = Insert {
        value: true,
        call_follows: false,
    };

    #[test]
    fn props_complete_for_the_type_their_block_is_for() {
        let dir = std::env::temp_dir().join(format!("som_lsp_prop_items_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "struct Vector<N: usize, T> { values: [T; N] }\n\
                    impl<T> Vector<2, T> {\n    prop x = self.values[0];\n}\n\
                    impl<T> Vector<3, T> {\n    prop z = self.values[2];\n    prop first: T = self.values[0];\n}\n\
                    fn main() -> i32 {\n    let v: Vector<3, i32> = Vector { values: [1, 2, 3] };\n    v.z\n}\n";
        std::fs::write(dir.join("main.som"), text).unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        let v = (a.ast.stmts.iter())
            .find_map(|(id, stmt)| match stmt {
                Stmt::Let { name, .. } if &*name.name == "v" => a.names.local(id),
                _ => None,
            })
            .unwrap();
        let owner = (a.ast.structs.iter())
            .map(|(s, _)| a.names.strukt(s))
            .next()
            .unwrap();
        let file = dir.join("main.som").to_string_lossy().into_owned();
        let viewer = viewer_at(&a, &file, text.len() - 3);
        let mut items = Vec::new();
        prop_items(&a, owner, a.typed.local(v), viewer, &mut items);
        let shown: Vec<(String, Option<CompletionItemKind>, Option<String>)> = (items.into_iter())
            .map(|i| (i.label, i.kind, i.detail))
            .collect();
        // Not `x`, which `Vector<2, T>` has.
        let prop = Some(CompletionItemKind::PROPERTY);
        assert_eq!(
            shown,
            [
                ("first".into(), prop, Some("i32".into())),
                ("z".into(), prop, Some("i32".into())),
            ]
        );
    }

    #[test]
    fn members_only_some_instances_have_complete_where_they_do() {
        let dir = std::env::temp_dir().join(format!("som_lsp_member_if_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = "struct Row<N: usize> { cells: [i32; N] }\n\
                    impl<N: usize> Row<N> {\n    @if N >= 3 {\n        prop third = .cells[2];\n    } \
                    else {\n        prop short = true;\n    }\n    @if N == 2 {\n        prop pair = true;\n    }\n}\n\
                    fn main() -> i32 {\n    let two = Row { cells: [1, 2] };\n    \
                    let three = Row { cells: [1, 2, 3] };\n    three.third\n}\n";
        std::fs::write(dir.join("main.som"), text).unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        let local = |wanted: &str| {
            (a.ast.stmts.iter())
                .find_map(|(id, stmt)| match stmt {
                    Stmt::Let { name, .. } if &*name.name == wanted => a.names.local(id),
                    _ => None,
                })
                .unwrap()
        };
        let owner = (a.ast.structs.iter())
            .map(|(s, _)| a.names.strukt(s))
            .next()
            .unwrap();
        let file = dir.join("main.som").to_string_lossy().into_owned();
        let viewer = viewer_at(&a, &file, text.len() - 3);
        let labels = |name: &str| {
            let mut items = Vec::new();
            prop_items(&a, owner, a.typed.local(local(name)), viewer, &mut items);
            let mut labels: Vec<String> = items.into_iter().map(|i| i.label).collect();
            labels.sort();
            labels
        };
        assert_eq!(labels("two"), ["pair", "short"]);
        assert_eq!(labels("three"), ["third"]);
    }

    #[test]
    fn members_hidden_from_here_are_not_completed() {
        let dir = std::env::temp_dir().join(format!("som_lsp_hidden_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("helper.som"),
            "pub struct Hello { private: i32, pub public: i32 }\n\
             impl Hello {\n    prop secret = .private;\n    pub prop open = .public;\n}\n",
        )
        .unwrap();
        let text =
            "fn main() -> i32 {\n    let h = Hello { private: 1, public: 2 };\n    h.public\n}\n";
        std::fs::write(dir.join("main.som"), text).unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        let helper = dir.join("helper.som").to_string_lossy().into_owned();
        let main = dir.join("main.som").to_string_lossy().into_owned();
        std::fs::remove_dir_all(&dir).unwrap();
        let s = a.ast.structs.iter().next().unwrap().0;
        let owner = a.names.strukt(s);
        let ty = a.typed.local(
            (a.ast.stmts.iter())
                .find_map(|(id, stmt)| match stmt {
                    Stmt::Let { name, .. } if &*name.name == "h" => a.names.local(id),
                    _ => None,
                })
                .unwrap(),
        );
        let completed = |file: &str| {
            let viewer = viewer_at(&a, file, 0);
            let mut shown: Vec<String> = (0..2)
                .filter(|&i| visibility::hidden_field(&a.ast, &a.names, s, i, viewer).is_none())
                .map(|i| a.ast.structs[s].fields[i].name.name.to_string())
                .collect();
            let mut items = Vec::new();
            prop_items(&a, owner, ty, viewer, &mut items);
            shown.extend(items.into_iter().map(|i| i.label));
            shown
        };
        // Another file sees only what's `pub`; its own file sees everything.
        assert_eq!(completed(&main), ["public", "open"]);
        assert_eq!(completed(&helper), ["private", "public", "open", "secret"]);
    }

    #[test]
    fn aliases_complete_like_the_struct_they_stand_for() {
        let dir = std::env::temp_dir().join(format!("som_lsp_alias_items_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.som");
        let text = "struct Vector<N: usize, T> { values: [T; N] }\n\
                    type Vector3d = Vector<3, f64>;\n\
                    type Also = Vector3d;\n\
                    type Number = i32;\n\
                    fn main() -> i32 { 0 }\n";
        std::fs::write(&main, text).unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        let file = main.to_string_lossy().to_string();
        let shown: Vec<(String, Option<String>, Option<String>)> =
            (module_items(&a, 0, None, VALUE).into_iter())
                .filter(|i| !i.label.starts_with("Vector "))
                .map(|i| (i.label, i.detail, i.insert_text))
                .collect();
        let vector = find_struct(&a, "Vector", &file, 0);
        let through_aliases = (
            find_struct(&a, "Vector3d", &file, 0),
            find_struct(&a, "Also", &file, 0),
        );
        let number = find_struct(&a, "Number", &file, 0);
        std::fs::remove_dir_all(&dir).unwrap();
        // The name alone, which can go on to `::new()`, then the literal.
        let alias = |name: &str, to: &str| Some(format!("type {name} = {to}"));
        let literal = |name: &str| Some(format!("{name} {{ values: $1 }}$0"));
        assert_eq!(
            shown,
            [
                ("Vector".into(), None, None),
                ("Vector3d".into(), alias("Vector3d", "Vector<3, f64>"), None),
                (
                    "Vector3d { … }".into(),
                    alias("Vector3d", "Vector<3, f64>"),
                    literal("Vector3d")
                ),
                ("Also".into(), alias("Also", "Vector3d"), None),
                (
                    "Also { … }".into(),
                    alias("Also", "Vector3d"),
                    literal("Also")
                ),
                ("Number".into(), alias("Number", "i32"), None),
            ]
        );
        // `Vector3d::` and `Vector3d { ` complete `Vector`'s methods and fields.
        assert!(vector.is_some());
        assert_eq!(through_aliases, (vector, vector));
        assert_eq!(number, None);
    }

    #[test]
    fn constants_complete_with_their_types_and_docs() {
        let dir = std::env::temp_dir().join(format!("som_lsp_consts_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.som"),
            "/// Cells across.\n@const COLS = 10;\n@const SCALE: f32 = 2.5;\n\
             @const CELLS = COLS * 20;\nfn main() -> i32 = COLS;\n",
        )
        .unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        let shown: Vec<(String, Option<CompletionItemKind>, Option<String>, bool)> =
            (module_items(&a, 0, None, VALUE).into_iter())
                .map(|i| (i.label, i.kind, i.detail, i.documentation.is_some()))
                .collect();
        let constant = Some(CompletionItemKind::CONSTANT);
        assert_eq!(
            shown,
            [
                (
                    "COLS".into(),
                    constant,
                    Some("@const COLS: i32 = 10".into()),
                    true
                ),
                (
                    "SCALE".into(),
                    constant,
                    Some("@const SCALE: f32 = 2.5".into()),
                    false
                ),
                (
                    "CELLS".into(),
                    constant,
                    Some("@const CELLS: i32 = COLS * 20  // 200".into()),
                    false
                ),
            ]
        );
    }

    #[test]
    fn a_library_used_from_two_files_lists_its_functions_once() {
        let dir = std::env::temp_dir().join(format!("som_lsp_libc_twice_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("main.som"),
            "extern \"c\";\nfn main() -> i32 = c::abs(-1);\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("more.som"),
            "extern \"c\";\nfn other() -> i32 = c::abs(2);\n",
        )
        .unwrap();
        let a = som_driver::analyze(&dir, &som_driver::Options::default());
        std::fs::remove_dir_all(&dir).unwrap();
        let c = a.ast.modules.iter().position(|m| &*m.name == "c").unwrap();
        let labels: Vec<String> = (module_items(&a, c, Some("c"), VALUE).into_iter())
            .map(|i| i.label)
            .filter(|l| l == "c::abs")
            .collect();
        assert_eq!(labels, ["c::abs"]);
    }

    #[test]
    fn out_parameters_complete_as_out() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn divide(a: i32, b: i32, out remainder: i32) -> i32 {\n    \
                      remainder = a % b;\n    a / b\n}\n\n\
                      fn main() { let q = divide(17, 5, out remainder); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let divide = a.ast.funcs.iter().find(|(_, f)| &*f.name.name == "divide");
        let item = func_item(&a, divide.unwrap().0, "divide".into(), VALUE);
        assert_eq!(
            item.insert_text.as_deref(),
            Some("divide(${1:a}, ${2:b}, out ${3:remainder})$0")
        );
    }
}

//! Who can see what. `pub`: everyone. `mod`: the item's module, the folder it's in. Without
//! either: its own file, and for a member (a field, or a method or property of an impl block)
//! also its type's impl blocks, wherever they are. Making a struct with `Name { … }` sets each
//! of its fields, so each has to be visible, as in Rust. A trait's methods, and those of its implementations, are
//! as visible as the trait. What isn't an item (a variable, a type parameter) is always seen.

use som_ast::{Ast, Id, Item, Struct, Visibility};
use som_base::message::code;
use som_base::{Fix, Span, msg};

use crate::names::{DefId, DefKind, Names};

/// Where a name is used: its module, its file, and the type whose impl block it's in, if any.
#[derive(Clone, Copy)]
pub struct Viewer<'a> {
    pub module: usize,
    pub file: &'a str,
    pub owner: Option<DefId>,
}

/// What limits who sees an item: its visibility, and for a member, its type.
#[derive(Clone, Copy, Debug)]
pub struct Access {
    pub visibility: Visibility,
    /// The type it's a method or property of.
    pub owner: Option<DefId>,
}

/// What limits who sees `def`; `None` if anyone can.
pub fn access(ast: &Ast, names: &Names, def: DefId) -> Option<Access> {
    let item = |visibility| {
        Some(Access {
            visibility,
            owner: None,
        })
    };
    match names.def(def).kind {
        DefKind::Func(f) => {
            let func = &ast.funcs[f];
            if func.in_trait {
                return None;
            }
            match names.impl_of(f) {
                // An implementation of a trait has what the trait promises: as visible as it.
                Some(i) if names.impls[i].trait_.is_some() => None,
                Some(i) => Some(Access {
                    visibility: func.visibility,
                    owner: names.impls[i].owner,
                }),
                None => item(func.visibility),
            }
        }
        DefKind::Struct(s) => item(ast.structs[s].visibility),
        // As visible as the enum.
        DefKind::Variant { strukt, .. } => item(ast.structs[strukt].visibility),
        DefKind::Trait(t) => item(ast.traits[t].visibility),
        DefKind::Alias { ty } => (ast.modules.iter())
            .flat_map(|m| &m.items)
            .find_map(|i| match i {
                Item::Alias(alias) if alias.ty == ty => Some(alias.visibility),
                _ => None,
            })
            .and_then(item),
        _ => None,
    }
}

/// Why something can't be seen from where it's used, for saying so.
#[derive(Clone, Debug)]
pub struct Hidden {
    pub limit: Limit,
    pub defined_at: Span,
    pub fix: Option<Fix>,
}

/// Whether code at `viewer` can see `def`.
pub fn can_see(ast: &Ast, names: &Names, def: DefId, viewer: Viewer) -> bool {
    hidden(ast, names, def, viewer).is_none()
}

/// Why `def` can't be seen at `viewer`; `None` if it can.
pub fn hidden(ast: &Ast, names: &Names, def: DefId, viewer: Viewer) -> Option<Hidden> {
    let access = access(ast, names, def)?;
    let defined = names.def(def);
    let item = match defined.kind {
        DefKind::Func(f) => Some(ast.funcs.span(f)),
        DefKind::Struct(s) | DefKind::Variant { strukt: s, .. } => Some(ast.structs.span(s)),
        DefKind::Trait(t) => Some(ast.traits.span(t)),
        _ => None,
    };
    check(
        ast,
        names,
        access,
        defined.module,
        &defined.span,
        item,
        viewer,
    )
}

/// Why field `index` of struct `s` can't be seen at `viewer`; `None` if it can. Without
/// `pub` or `mod`, a field is its struct's: only its impl blocks see it.
pub fn hidden_field(
    ast: &Ast,
    names: &Names,
    s: Id<Struct>,
    index: usize,
    viewer: Viewer,
) -> Option<Hidden> {
    let field = &ast.structs[s].fields[index];
    let owner = names.strukt(s);
    let access = Access {
        visibility: field.visibility,
        owner: Some(owner),
    };
    let at = &field.name.span;
    check(
        ast,
        names,
        access,
        names.def(owner).module,
        at,
        Some(at),
        viewer,
    )
}

/// Whether something with `access`, defined at `defined_at` in `module`, is hidden from
/// `viewer`. `item` is where its declaration starts, where `pub` or `mod` would go.
fn check(
    ast: &Ast,
    names: &Names,
    access: Access,
    module: usize,
    defined_at: &Span,
    item: Option<&Span>,
    viewer: Viewer,
) -> Option<Hidden> {
    let file = defined_at.source.identifier();
    let limit = match (access.visibility, access.owner) {
        (Visibility::Public, _) => return None,
        (Visibility::Module, _) if module == viewer.module => return None,
        (Visibility::Private, _) if file == viewer.file => return None,
        (Visibility::Private, Some(owner)) if viewer.owner == Some(owner) => return None,
        (Visibility::Module, _) => Limit::Module(ast.modules[module].name.to_string()),
        (Visibility::Private, owner) => {
            let file = std::path::Path::new(file)
                .file_name()
                .map_or(file.to_string(), |n| n.to_string_lossy().into_owned());
            match owner {
                Some(owner) => Limit::Type(file, names.def(owner).name.to_string()),
                None => Limit::File(file),
            }
        }
    };
    Some(Hidden {
        limit,
        defined_at: defined_at.clone(),
        fix: item.map(|item| fix(item, module == viewer.module)),
    })
}

/// What something is limited to, for saying so: its file, its module, or a member's file
/// and type.
#[derive(Clone, Debug)]
pub enum Limit {
    File(String),
    Module(String),
    Type(String, String),
}

/// A fix for something whose declaration starts at `item`: `mod` where it's used in the
/// same module, `pub` otherwise, put in front of it, or in place of the `mod` it has.
fn fix(item: &Span, same_module: bool) -> Fix {
    let keyword = if same_module { "mod" } else { "pub" };
    let content = item.source.content();
    let before = content[..item.start_offset].trim_end();
    let edit = if keyword == "pub" && before.ends_with("mod") {
        let at = before.len() - 3..before.len();
        (Span::from_range(at, item.source.clone()), "pub".to_string())
    } else {
        (item.before(), format!("{keyword} "))
    };
    Fix {
        title: msg!["make it ", code(keyword)],
        edits: vec![edit],
    }
}

/// The error for `name`, used at `span` but hidden from there.
pub fn diagnostic(name: &str, span: &Span, hidden: &Hidden) -> som_base::Diagnostic {
    use som_base::{Diagnostic, Label};
    let (message, defined) = match &hidden.limit {
        Limit::File(file) => (
            msg![code(name), " is private to ", code(file)],
            msg!["defined here, without ", code("pub"), " or ", code("mod")],
        ),
        Limit::Module(module) => (
            msg![code(name), " is only usable in module ", code(module)],
            msg!["defined here as ", code("mod")],
        ),
        Limit::Type(file, owner) => (
            msg![code(name), " is private to ", code(owner)],
            msg![
                "defined here: only ",
                code(file),
                " and ",
                code(owner),
                "'s impl blocks can use it"
            ],
        ),
    };
    let d = Diagnostic::error(message)
        .with_label(Label::primary(span.clone(), "used here"))
        .with_label(Label::secondary(hidden.defined_at.clone(), defined));
    match &hidden.fix {
        Some(fix) => d.with_fix(fix.clone()),
        None => d,
    }
}

use som_ast::Item;
use tower_lsp::lsp_types::CompletionItem;

use super::Request;
use super::items::{alias_items, trait_item, type_item};
use crate::lookup::modules_in_scope;
use crate::text_scan::{TypeSlot, type_slot};

/// `impl ` lists traits and types, `impl Add for ` types, `<T: ` traits: from the modules in
/// scope by name, from others by theirs.
pub(super) fn complete(request: &Request) -> Option<Vec<CompletionItem>> {
    let slot = type_slot(request.text, request.offset)?;
    let a = request.a;
    let (here, in_scope) = modules_in_scope(a, request.file, request.offset);
    let wants_traits = slot != TypeSlot::Type;
    let wants_types = slot != TypeSlot::Trait;
    let mut items = Vec::new();
    for (m, module) in a.ast.modules.iter().enumerate() {
        let qualify = (!in_scope.contains(&m) && m != here && !module.name.is_empty())
            .then_some(&*module.name);
        let name = |n: &str| match qualify {
            Some(module) => format!("{module}::{n}"),
            None => n.to_string(),
        };
        for item in &module.items {
            match item {
                Item::Trait(t) if wants_traits => {
                    items.push(trait_item(a, *t, name(&a.ast.traits[*t].name.name)));
                }
                Item::Struct(s) if wants_types => {
                    items.push(type_item(a, *s, name(&a.ast.structs[*s].name.name)));
                }
                Item::Alias(alias) if wants_types => {
                    items.extend(alias_items(a, alias, name(&alias.name.name), None));
                }
                _ => {}
            }
        }
    }
    Some(items)
}

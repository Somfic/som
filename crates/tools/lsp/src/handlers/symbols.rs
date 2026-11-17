use som_ast::{Func, Id, Impl, Item, Span, Struct};
use som_driver::Analysis;
use som_resolve::DefKind;
use tower_lsp::lsp_types::{DocumentSymbol, SymbolInformation, SymbolKind};

use crate::convert;
use crate::docs::{signature, ty, variant_text};
use crate::index::items_in_file;

/// The outline of `file`: its items, with a struct's fields, an enum's variants and an impl
/// block's or trait's methods under them.
pub(crate) fn document_symbols(a: &Analysis, file: &str) -> Vec<DocumentSymbol> {
    let ast = &a.ast;
    let mut symbols = Vec::new();
    for item in items_in_file(ast, file) {
        match item {
            Item::Func(id) => symbols.push(func_symbol(a, *id, SymbolKind::FUNCTION)),
            Item::Extern(block) => symbols.extend(
                block
                    .funcs
                    .iter()
                    .map(|&id| func_symbol(a, id, SymbolKind::FUNCTION)),
            ),
            Item::Struct(id) if ast.structs[*id].variants.is_some() => {
                symbols.push(enum_symbol(a, *id));
            }
            Item::Struct(id) => symbols.push(struct_symbol(a, *id)),
            Item::Impl(block) => symbols.push(impl_symbol(a, block)),
            Item::Trait(id) => {
                let t = &ast.traits[*id];
                let methods: Vec<DocumentSymbol> = (t.methods.iter())
                    .map(|&m| func_symbol(a, m, SymbolKind::METHOD))
                    .collect();
                symbols.push(symbol(
                    t.name.name.to_string(),
                    None,
                    SymbolKind::INTERFACE,
                    ast.traits.span(*id),
                    &t.name.span,
                    Some(methods),
                ));
            }
            Item::Alias(alias) => {
                let span = alias.name.span.merge(ast.types.span(alias.ty));
                symbols.push(symbol(
                    alias.name.name.to_string(),
                    Some(ast.type_text(alias.ty)),
                    SymbolKind::TYPE_PARAMETER,
                    &span,
                    &alias.name.span,
                    None,
                ));
            }
            Item::Const(id) => {
                let c = &ast.consts[*id];
                let def = a.names.const_def(*id);
                symbols.push(symbol(
                    c.name.name.to_string(),
                    a.typed.try_local(def).map(|t| ty(a, t)),
                    SymbolKind::CONSTANT,
                    ast.consts.span(*id),
                    &c.name.span,
                    None,
                ));
            }
            Item::Use(_) => {}
        }
    }
    symbols
}

fn func_symbol(a: &Analysis, id: Id<Func>, kind: SymbolKind) -> DocumentSymbol {
    let ast = &a.ast;
    symbol(
        ast.funcs[id].name.name.to_string(),
        Some(signature(a, id)),
        kind,
        ast.funcs.span(id),
        &ast.funcs[id].name.span,
        None,
    )
}

fn enum_symbol(a: &Analysis, id: Id<Struct>) -> DocumentSymbol {
    let s = &a.ast.structs[id];
    let variants = (s.variants.iter().flatten().enumerate())
        .map(|(i, v)| {
            symbol(
                v.name.name.to_string(),
                Some(variant_text(a, id, i)),
                SymbolKind::ENUM_MEMBER,
                &v.name.span,
                &v.name.span,
                None,
            )
        })
        .collect();
    symbol(
        s.name.name.to_string(),
        None,
        SymbolKind::ENUM,
        a.ast.structs.span(id),
        &s.name.span,
        Some(variants),
    )
}

fn struct_symbol(a: &Analysis, id: Id<Struct>) -> DocumentSymbol {
    let s = &a.ast.structs[id];
    let fields = s
        .fields
        .iter()
        .zip(a.typed.fields(id))
        .map(|(f, &t)| {
            let span = f.name.span.merge(a.ast.types.span(f.ty));
            symbol(
                f.name.name.to_string(),
                Some(ty(a, t)),
                SymbolKind::FIELD,
                &span,
                &f.name.span,
                None,
            )
        })
        .collect();
    symbol(
        s.name.name.to_string(),
        None,
        SymbolKind::STRUCT,
        a.ast.structs.span(id),
        &s.name.span,
        Some(fields),
    )
}

fn impl_symbol(a: &Analysis, block: &Impl) -> DocumentSymbol {
    let ast = &a.ast;
    let methods: Vec<DocumentSymbol> = block
        .methods
        .iter()
        .map(|&m| {
            let kind = if ast.funcs[m].prop {
                SymbolKind::PROPERTY
            } else {
                SymbolKind::METHOD
            };
            func_symbol(a, m, kind)
        })
        .collect();
    let span = block.methods.last().map_or(block.target.span(), |&m| {
        block.target.span().merge(ast.funcs.span(m))
    });
    symbol(
        format!("impl {}", block.target),
        None,
        SymbolKind::OBJECT,
        &span,
        &block.target.span(),
        Some(methods),
    )
}

fn symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    span: &Span,
    name_span: &Span,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    #[allow(deprecated)]
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: convert::span_to_range(span),
        selection_range: convert::span_to_range(name_span),
        children: children.filter(|c| !c.is_empty()),
    }
}

/// Whether `name` holds `query`'s letters in order, ignoring case: `mpins` matches
/// `Map::insert`.
fn fuzzy_matches(query: &[char], name: &str) -> bool {
    let mut wanted = query.iter().peekable();
    for c in name.to_lowercase().chars() {
        if wanted.peek() == Some(&&c) {
            wanted.next();
        }
    }
    wanted.peek().is_none()
}

/// The program's functions, types and traits whose names match `query`: its own first, then
/// the standard library's.
pub(crate) fn workspace_symbols(a: &Analysis, query: &str) -> Vec<SymbolInformation> {
    let query: Vec<char> = query.to_lowercase().chars().collect();
    let mut found = Vec::new();
    for (_, def) in a.names.defs.iter_with_ids() {
        let (kind, container) = match def.kind {
            DefKind::Func(f) => {
                let container = (a.names.impl_of(f))
                    .and_then(|i| a.names.impls[i].owner)
                    .map(|owner| a.names.def(owner).name.to_string());
                let kind = if a.ast.funcs[f].prop {
                    SymbolKind::PROPERTY
                } else if container.is_some() || a.ast.funcs[f].in_trait {
                    SymbolKind::METHOD
                } else {
                    SymbolKind::FUNCTION
                };
                (kind, container)
            }
            DefKind::Struct(s) if a.ast.structs[s].variants.is_some() => (SymbolKind::ENUM, None),
            DefKind::Struct(_) => (SymbolKind::STRUCT, None),
            DefKind::Variant { strukt, .. } => (
                SymbolKind::ENUM_MEMBER,
                Some(a.ast.structs[strukt].name.name.to_string()),
            ),
            DefKind::Trait(_) => (SymbolKind::INTERFACE, None),
            DefKind::Alias { .. } => (SymbolKind::TYPE_PARAMETER, None),
            _ => continue,
        };
        let name = def.name.to_string();
        let full = match &container {
            Some(c) => format!("{c}::{name}"),
            None => name.clone(),
        };
        if def.span.length == 0
            || def.span.source.identifier().starts_with('<')
            || !fuzzy_matches(&query, &full)
        {
            continue;
        }
        let Some(location) = convert::location(&def.span) else {
            continue;
        };
        #[allow(deprecated)]
        found.push(SymbolInformation {
            name,
            kind,
            tags: None,
            deprecated: None,
            location,
            container_name: container,
        });
    }
    found.sort_by_key(|s| {
        (s.location.uri.to_file_path()).is_ok_and(|p| som_driver::stdlib::contains(&p))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{POINTS, analyzed};

    #[test]
    fn workspace_symbols_match_letters_in_order() {
        let (_dir, _file, a) = analyzed(POINTS);
        let names: Vec<String> = (workspace_symbols(&a, "mke").into_iter())
            .map(|s| s.name)
            .collect();
        assert_eq!(names.first().map(String::as_str), Some("make"), "{names:?}");
    }
}

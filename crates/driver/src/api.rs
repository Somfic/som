//! `som api`: what a program or library lets others use, one line each, with what each
//! function giving a `T!` fails with spelled out, worked out or not. Kept next to the code,
//! a change to it shows in review, even one made deep inside that no signature shows.

use som_ast::{Ast, Func, Id, Item, Visibility};
use som_resolve::Names;
use som_typeck::Typed;
use std::path::Path;

/// The `pub` items of the modules loaded from `root`, a file or a directory, in the order
/// they're written: each module's after a line naming it.
pub fn api(ast: &Ast, names: &Names, typed: &Typed, root: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut out = String::new();
    for module in &ast.modules {
        if som_lang::is_std_module(&module.name) {
            continue;
        }
        let path = module.path.canonicalize().unwrap_or_else(|_| module.path.clone());
        if !path.starts_with(&root) && !root.starts_with(&path) {
            continue;
        }
        let mut lines = Vec::new();
        for item in &module.items {
            match item {
                Item::Func(f) if public(ast, *f) => lines.push(func_line(ast, names, typed, *f)),
                Item::Struct(s) if ast.structs[*s].visibility == Visibility::Public => {
                    let strukt = &ast.structs[*s];
                    let what = if strukt.is_enum() { "enum" } else { "struct" };
                    lines.push(format!("pub {what} {}", strukt.name));
                }
                Item::Impl(block) => {
                    let methods: Vec<String> = (block.methods.iter())
                        .filter(|&&m| public(ast, m))
                        .map(|&m| format!("    {}", func_line(ast, names, typed, m)))
                        .collect();
                    if !methods.is_empty() {
                        let target = match &block.trait_ {
                            Some((trait_, _)) => format!("{trait_} for {}", block.target),
                            None => block.target.to_string(),
                        };
                        lines.push(format!("impl {target} {{\n{}\n}}", methods.join("\n")));
                    }
                }
                _ => {}
            }
        }
        if lines.is_empty() {
            continue;
        }
        if !module.name.is_empty() {
            out.push_str(&format!("mod {}\n", module.name));
        }
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

fn public(ast: &Ast, f: Id<Func>) -> bool {
    ast.funcs[f].visibility == Visibility::Public
}

/// `pub fn load(path: string) -> i64!(NotFound, ParseError)`.
fn func_line(ast: &Ast, names: &Names, typed: &Typed, f: Id<Func>) -> String {
    let func = &ast.funcs[f];
    let sig = typed.func(f);
    let params: Vec<String> = (func.params.iter().zip(&sig.params))
        .map(|(p, &t)| format!("{}: {}", p.name, typed.table.display(t, names)))
        .collect();
    let generics: Vec<String> = func.generics.iter().map(|g| g.name.name.to_string()).collect();
    let generics = match generics.is_empty() {
        true => String::new(),
        false => format!("<{}>", generics.join(", ")),
    };
    let ret = som_typeck::failures::returns_text(typed, names, f);
    let ret = match ret.as_str() {
        "()" => String::new(),
        _ => format!(" -> {ret}"),
    };
    format!("pub fn {}{generics}({}){ret}", func.name, params.join(", "))
}

/// What changed between `before` and `after`, written by `api`: the lines gone, then those
/// new. Changed lines are both. `None` if nothing did.
pub fn api_changes(before: &str, after: &str) -> Option<(Vec<String>, Vec<String>)> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let gone: Vec<String> = (old.iter()).filter(|l| !new.contains(l)).map(|l| l.to_string()).collect();
    let added: Vec<String> = (new.iter()).filter(|l| !old.contains(l)).map(|l| l.to_string()).collect();
    (!gone.is_empty() || !added.is_empty()).then_some((gone, added))
}

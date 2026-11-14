//! What an `impl Trait for …` block still has to write: each associated type and method the
//! trait has that it doesn't, as source text to insert. For the error's fix, and an editor's
//! completions inside the block.

use som_ast::{Ast, Func, Id, Impl, TypeExpr};
use som_resolve::{DefKind, Names};

/// An item an implementation is missing.
#[derive(Clone, Debug)]
pub struct Stub {
    /// Its name: `eq`, `Item`.
    pub name: String,
    /// Whether it's an associated type, rather than a method.
    pub is_type: bool,
    /// How it's shown: `fn eq(self: &Hello, other: &Hello) -> bool`, `type Item`.
    pub signature: String,
    /// Its text with an empty body, one level indented, ending in a newline.
    pub text: String,
    /// Its text as a snippet: tab stops for each parameter's name and the body.
    pub snippet: String,
}

/// The items impl block `block`, the `index`th, is missing from its trait: associated types
/// first, then methods, in the trait's order. A method with a body in the trait isn't missing.
pub fn missing(ast: &Ast, names: &Names, index: usize, block: &Impl) -> Vec<Stub> {
    let Some(trait_def) = names.impls.get(index).and_then(|i| i.trait_) else {
        return Vec::new();
    };
    let DefKind::Trait(t) = names.def(trait_def).kind else {
        return Vec::new();
    };
    let target = match block.target_args.is_empty() {
        true => block.target.to_string(),
        false => format!(
            "{}<{}>",
            block.target,
            (block.target_args.iter().map(|&a| ast.type_text(a))).collect::<Vec<_>>().join(", ")
        ),
    };
    let trait_args: Vec<String> = (block.trait_.iter())
        .flat_map(|(_, args)| args)
        .map(|&a| ast.type_text(a))
        .collect();
    let written = |name: &str, is_type: bool| match is_type {
        true => block.types.iter().any(|(n, _)| &*n.name == name),
        false => (block.methods.iter()).any(|&m| &*ast.funcs[m].name.name == name),
    };
    stubs(ast, t, &target, &trait_args, &written)
}

/// The items an implementation of trait `t` for `target` (as written) has to write, with the
/// trait's parameters `trait_args`, but not the ones `written` says it has.
pub fn stubs(
    ast: &Ast,
    t: Id<som_ast::Trait>,
    target: &str,
    trait_args: &[String],
    written: &dyn Fn(&str, bool) -> bool,
) -> Vec<Stub> {
    let tr = &ast.traits[t];
    // What the trait's names stand for here: `Self` the target, its parameters what the
    // implementation gives them.
    let mut replace: Vec<(String, String)> = vec![("Self".into(), target.to_string())];
    for (generic, arg) in tr.generics.iter().zip(trait_args) {
        replace.push((generic.name.name.to_string(), arg.clone()));
    }
    let fill = |ty: Id<TypeExpr>| {
        // `Self::Item` is the implementation's `Item`, named by itself inside it.
        let text = ast.type_text(ty).replace("Self::", "");
        substitute(&text, &replace)
    };

    let mut stubs = Vec::new();
    for assoc in &tr.types {
        let name = assoc.name.name.to_string();
        if written(&name, true) {
            continue;
        }
        stubs.push(Stub {
            signature: format!("type {name}"),
            text: format!("    type {name} = ;\n"),
            snippet: format!("type {name} = ${{1}};$0"),
            name,
            is_type: true,
        });
    }
    for &method in &tr.methods {
        let func: &Func = &ast.funcs[method];
        if written(&func.name.name, false) || func.body.is_some() {
            continue;
        }
        let name = func.name.name.to_string();
        // Its own generics: after `Self` and the trait's.
        let own: Vec<String> = (func.generics.iter().skip(1 + tr.generics.len()))
            .map(|g| {
                let mut text = g.name.name.to_string();
                if let Some(ty) = g.ty {
                    text += &format!(": {}", fill(ty));
                } else if !g.bounds.is_empty() {
                    let bounds: Vec<String> = g.bounds.iter().map(|b| b.to_string()).collect();
                    text += &format!(": {}", bounds.join(" + "));
                }
                text
            })
            .collect();
        let generics = match own.is_empty() {
            true => String::new(),
            false => format!("<{}>", own.join(", ")),
        };
        let params: Vec<(String, String)> = (func.params.iter())
            .map(|p| (p.name.name.to_string(), p.ty.map(fill).unwrap_or_default()))
            .collect();
        let ret = func.returns.map(|r| format!(" -> {}", fill(r))).unwrap_or_default();
        let shown: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
        let signature = format!("fn {name}{generics}({}){ret}", shown.join(", "));
        // `self` keeps its name; the others are tab stops, then the body.
        let mut stop = 0;
        let tabbed: Vec<String> = (params.iter())
            .map(|(n, t)| match n.as_str() {
                "self" => format!("self: {t}"),
                _ => {
                    stop += 1;
                    format!("${{{stop}:{n}}}: {t}")
                }
            })
            .collect();
        stubs.push(Stub {
            text: format!("    {signature} {{\n        \n    }}\n"),
            snippet: format!(
                "fn {name}{generics}({}){ret} {{\n\t$0\n}}",
                tabbed.join(", ")
            ),
            signature,
            name,
            is_type: false,
        });
    }
    stubs
}

/// `text` with each whole word in `replace` replaced: `Self` in `&Self`, not in `Selfish`.
fn substitute(text: &str, replace: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        match replace.iter().find(|(from, _)| from == word) {
            Some((_, to)) => out.push_str(to),
            None => out.push_str(word),
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

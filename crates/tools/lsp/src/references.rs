//! Where each symbol is named in a program: its definition, and every use. Finding
//! references, highlighting a symbol's other uses, and renaming are all built on this.

use std::collections::HashSet;

use som_ast::{Expr, Item, Pattern, TypeExpr};
use som_base::Span;
use som_driver::Analysis;
use som_resolve::{DefId, DefKind};

use crate::types::struct_of;

/// Something a name can name: a definition, or a struct's field, by its struct and index,
/// which isn't a definition of its own.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Symbol {
    Def(DefId),
    Field(DefId, usize),
}

#[derive(Clone, Debug)]
pub struct Occurrence {
    pub symbol: Symbol,
    pub span: Span,
    /// Where it's defined, rather than used.
    pub definition: bool,
    /// `Point { x }`: the field `x` given the variable `x`, both named once.
    pub shorthand: bool,
}

/// Every place a symbol is named in `a`'s program, each once. Only where the text there is
/// the symbol's own name: not `Self` for a struct, or code the compiler wrote.
pub fn occurrences(a: &Analysis) -> Vec<Occurrence> {
    let (ast, names, typed) = (&a.ast, &a.names, &a.typed);
    let mut found = Vec::new();
    let mut add = |symbol: Symbol, span: &Span, definition: bool, shorthand: bool| {
        found.push(Occurrence {
            symbol,
            span: span.clone(),
            definition,
            shorthand,
        });
    };

    for (id, def) in names.defs.iter_with_ids() {
        add(Symbol::Def(id), &def.span, true, false);
    }
    for (s, strukt) in ast.structs.iter() {
        if strukt.variants.is_none() {
            let def = names.strukt(s);
            for (i, field) in strukt.fields.iter().enumerate() {
                add(Symbol::Field(def, i), &field.name.span, true, false);
            }
        }
    }

    for (id, expr) in ast.exprs.iter() {
        match expr {
            Expr::Var(path) => {
                if let (Some(def), Some(last)) = (names.expr(id), path.segments.last()) {
                    add(Symbol::Def(def), &last.span, false, false);
                }
            }
            Expr::Call { callee, .. } => {
                let def = typed.callee(id, names).or_else(|| names.expr(id));
                if let (Some(def), Some(last)) = (def, callee.segments.last()) {
                    add(Symbol::Def(def), &last.span, false, false);
                }
            }
            Expr::Construct { ty, fields, .. } => {
                let Some(def) = names.expr(id) else { continue };
                if let Some(last) = ty.segments.last() {
                    add(Symbol::Def(def), &last.span, false, false);
                }
                let DefKind::Struct(s) = names.def(def).kind else {
                    continue;
                };
                for (name, value) in fields {
                    let declared = &ast.structs[s].fields;
                    if let Some(i) = declared.iter().position(|f| f.name.name == name.name) {
                        // `Point { x }`: the field and the variable it's given, named once.
                        let shorthand =
                            ast.exprs.span(*value).start_offset == name.span.start_offset;
                        add(Symbol::Field(def, i), &name.span, false, shorthand);
                    }
                }
            }
            Expr::MethodCall { method, .. } => {
                if let Some(def) = typed.method(id) {
                    add(Symbol::Def(def), &method.span, false, false);
                }
            }
            Expr::Field { object, field } => {
                if let Some(def) = typed.method(id) {
                    // A property, `list.len`.
                    add(Symbol::Def(def), &field.span, false, false);
                } else if let (Some(i), Some(owner)) =
                    (typed.field(id), struct_of(a, typed.expr(*object)))
                {
                    add(Symbol::Field(owner, i), &field.span, false, false);
                }
            }
            _ => {}
        }
    }
    for (id, ty) in ast.types.iter() {
        if let TypeExpr::Named { path, .. } = ty
            && let (Some(def), Some(last)) = (names.ty(id), path.segments.last())
        {
            add(Symbol::Def(def), &last.span, false, false);
        }
    }
    for (id, pattern) in ast.patterns.iter() {
        if let Pattern::Variant { path, .. } = pattern
            && let (Some(def), Some(last)) = (names.pattern(id), path.segments.last())
        {
            add(Symbol::Def(def), &last.span, false, false);
        }
    }
    // `impl Hash for Point`: the trait and the type, in the order the resolver numbered them.
    let blocks = (ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Impl(block) => Some(block),
            _ => None,
        });
    for (block, info) in blocks.zip(&names.impls) {
        if block.target_ty.is_none()
            && let (Some(owner), Some(last)) = (info.owner, block.target.segments.last())
        {
            add(Symbol::Def(owner), &last.span, false, false);
        }
        if let (Some((path, _)), Some(trait_)) = (&block.trait_, info.trait_)
            && let Some(last) = path.segments.last()
        {
            add(Symbol::Def(trait_), &last.span, false, false);
        }
    }

    let mut seen = HashSet::new();
    found.retain(|o| {
        let name = name_of(a, o.symbol);
        o.span.length > 0
            && !o.span.source.identifier().starts_with('<')
            && *o.span.get_text() == *name
            && seen.insert((
                o.symbol,
                o.span.source.identifier().to_string(),
                o.span.start_offset,
            ))
    });
    found
}

pub fn name_of(a: &Analysis, symbol: Symbol) -> String {
    match symbol {
        Symbol::Def(def) => a.names.def(def).name.to_string(),
        Symbol::Field(owner, i) => match a.names.def(owner).kind {
            DefKind::Struct(s) => (a.ast.structs[s].fields.get(i))
                .map(|f| f.name.name.to_string())
                .unwrap_or_default(),
            _ => String::new(),
        },
    }
}

pub fn symbol_at(occurrences: &[Occurrence], file: &str, offset: usize) -> Option<Symbol> {
    occurrences
        .iter()
        .find(|o| {
            o.span.source.identifier() == file
                && (o.span.start_offset..=o.span.start_offset + o.span.length).contains(&offset)
        })
        .map(|o| o.symbol)
}

/// `symbol` and what has to keep its name with it: a trait's method and every
/// implementation of it, from whichever of them.
pub fn family(a: &Analysis, symbol: Symbol) -> Vec<Symbol> {
    let (ast, names) = (&a.ast, &a.names);
    let Symbol::Def(def) = symbol else {
        return vec![symbol];
    };
    let DefKind::Func(f) = names.def(def).kind else {
        return vec![symbol];
    };
    let trait_ = match names.impl_of(f) {
        Some(i) => names.impls[i].trait_,
        None => (ast.traits.iter())
            .find(|(_, t)| t.methods.contains(&f))
            .map(|(t, _)| names.trait_def(t)),
    };
    let Some(trait_) = trait_ else {
        return vec![symbol];
    };
    let name = &ast.funcs[f].name.name;
    (ast.funcs.iter())
        .filter(|(g, func)| {
            func.name.name == *name
                && match names.impl_of(*g) {
                    Some(i) => names.impls[i].trait_ == Some(trait_),
                    None => matches!(names.def(trait_).kind,
                        DefKind::Trait(t) if ast.traits[t].methods.contains(g)),
                }
        })
        .map(|(g, _)| Symbol::Def(names.func(g)))
        .collect()
}

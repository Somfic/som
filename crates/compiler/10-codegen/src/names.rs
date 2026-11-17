use som_mir::*;
use std::collections::{HashMap, HashSet};

/// C's keywords, the names `<stdbool.h>` and `<stdint.h>` define, and `main`.
const C_RESERVED: &[&str] = &[
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
    "bool",
    "true",
    "false",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "intptr_t",
    "uintptr_t",
    "main",
];

/// Picks `base`, or `base_2`, `base_3`... for the first one that isn't taken or reserved,
/// and takes it. Names starting with `_` are reserved in C, so those get `prefix` in front.
fn unique(
    base: &str,
    prefix: &str,
    taken: &mut HashSet<String>,
    reserved: impl Fn(&str) -> bool,
) -> String {
    // A name reserved however it's numbered, as anything starting `som_` is, gets the prefix
    // first: numbering it would never find one free.
    let base = if base.starts_with('_') || reserved(&format!("{base}_2")) {
        format!("{prefix}{base}")
    } else {
        base.to_string()
    };
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains(&name) || C_RESERVED.contains(&name.as_str()) || reserved(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    taken.insert(name.clone());
    name
}

/// A som name as a C identifier: `Point::sum` → `Point_sum`, `id<&i32>` → `id_ref_i32`.
pub(crate) fn identifier(name: &str) -> String {
    let spelled = name
        .replace("::", "_")
        .replace("&mut ", "mutref_")
        .replace('&', "ref_")
        .replace('?', "_opt")
        .replace('*', "ptr");
    let mut out = String::new();
    for c in spelled.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        // One `_` at a time, so `id<i32>` is `id_i32`, not `id_i32_`.
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    let trimmed = out.trim_end_matches('_');
    if trimmed.is_empty() {
        "f".into()
    } else {
        trimmed.into()
    }
}

pub(crate) struct Names {
    /// One for each set of type arguments: `Pair<i32>` is `Pair_i32`.
    pub structs: HashMap<(DefId, Vec<TypeId>), String>,
    /// The structs made for the optional, array, slice, text and range types the program uses.
    pub generated: HashMap<TypeId, String>,
    pub fields: HashMap<DefId, Vec<String>>,
    pub funcs: HashMap<FuncId, String>,
}

impl Names {
    pub fn new(mir: &Mir) -> Names {
        // Struct tags have their own namespace in C.
        let mut tags = HashSet::new();
        let mut structs = HashMap::new();
        let mut fields = HashMap::new();
        for s in &mir.structs {
            structs.insert(
                (s.def, s.args.clone()),
                unique(&identifier(&s.name), "s", &mut tags, |_| false),
            );
            let names = match &s.variants {
                None => {
                    let mut taken = HashSet::new();
                    (s.fields.iter())
                        .map(|(name, _)| unique(name, "f", &mut taken, |_| false))
                        .collect()
                }
                Some(variants) => enum_field_names(s, variants),
            };
            fields.insert(s.def, names);
        }
        let mut generated = HashMap::new();
        for ty in used_generated(mir) {
            let name = generated_name(mir, ty);
            generated.insert(ty, unique(&name, "s", &mut tags, |_| false));
        }

        // Externs keep their names, since that's what they link against; the rest avoid them.
        let externs: HashSet<String> = mir
            .funcs
            .iter()
            .filter_map(|f| f.link_name.clone())
            .collect();
        let mut taken = HashSet::new();
        let funcs = mir
            .funcs
            .iter_with_ids()
            .map(|(id, f)| match &f.link_name {
                Some(link) => (id, link.clone()),
                None if f.is_entry => (
                    id,
                    unique("som_main", "f", &mut taken, |n| externs.contains(n)),
                ),
                None => (
                    id,
                    unique(&identifier(&f.name), "f", &mut taken, |n| {
                        externs.contains(n)
                    }),
                ),
            })
            .collect();

        Names {
            structs,
            generated,
            fields,
            funcs,
        }
    }
}

/// An enum's members: `tag`, the common fields beside it, then each variant's own fields,
/// reached through the union of variants: `as.Rect.w`, `as.Circle._0`.
fn enum_field_names(s: &StructDef, variants: &[VariantDef]) -> Vec<String> {
    let mut taken_common: HashSet<String> = [som_mir::ENUM_TAG, "as"]
        .into_iter()
        .map(String::from)
        .collect();
    let mut names: Vec<String> = (s.fields.iter().enumerate())
        .map(|(i, (name, _))| match i {
            0 => som_mir::ENUM_TAG.to_string(),
            _ if !name.contains('.') => unique(name, "f", &mut taken_common, |_| false),
            _ => String::new(),
        })
        .collect();
    let mut taken_variants = HashSet::new();
    for (i, v) in variants.iter().enumerate() {
        let variant = unique(&v.name, "v", &mut taken_variants, |_| false);
        let mut taken = HashSet::new();
        for &index in s.variant_fields(i) {
            let name = &s.fields[index].0;
            let Some((_, field)) = name.split_once('.') else {
                continue;
            };
            // A position, `_0`, can't clash with a name.
            let field = match field.parse::<usize>() {
                Ok(n) => format!("_{n}"),
                Err(_) => unique(field, "f", &mut taken, |_| false),
            };
            names[index] = format!("as.{variant}.{field}");
        }
    }
    names
}

fn generated_name(mir: &Mir, ty: TypeId) -> String {
    match mir.types.get(ty) {
        Ty::Optional(inner) => format!("som_opt_{}", identifier(&mir.display_ty(*inner))),
        Ty::Array { element, len } => format!(
            "som_arr_{}_{}",
            identifier(&mir.display_ty(*element)),
            mir.types.len(*len)
        ),
        Ty::Slice(element) => format!("som_slice_{}", identifier(&mir.display_ty(*element))),
        Ty::Str => "som_str".to_string(),
        Ty::Range(int) => format!("som_range_{}", identifier(&mir.display_ty(*int))),
        _ => unreachable!("used_generated returns optionals, arrays, slices and text"),
    }
}

/// The optional, array, slice, text and range types in the program's functions and structs.
fn used_generated(mir: &Mir) -> Vec<TypeId> {
    let mut todo: Vec<TypeId> = Vec::new();
    for func in &mir.funcs {
        todo.extend(&func.params);
        todo.push(func.ret);
        if let Some(body) = &func.body {
            todo.extend(body.locals.iter().map(|l| l.ty));
        }
    }
    for s in &mir.structs {
        todo.extend(s.fields.iter().map(|&(_, ty)| ty));
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    while let Some(ty) = todo.pop() {
        if !seen.insert(ty) {
            continue;
        }
        let t = mir.types.get(ty);
        if matches!(
            t,
            Ty::Optional(_) | Ty::Array { .. } | Ty::Slice(_) | Ty::Str | Ty::Range(_)
        ) {
            found.push(ty);
        }
        todo.extend(t.children());
    }
    // Sorted, so the output doesn't depend on hash order.
    found.sort();
    found
}

/// C names for a function's locals, indexed by local id: the som name where possible. A
/// temporary holding what a call returns is named after the function, `Vector::x` as `x`;
/// the return value is `_0` and other temporaries are `_N`. Removed locals get an empty name.
pub(crate) fn local_names(mir: &Mir, body: &Body, functions: &HashSet<String>) -> Vec<String> {
    let mut callees: HashMap<Local, &str> = HashMap::new();
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        if let StatementKind::Assign(dest, Rvalue::Call { func, .. }) = &stmt.kind
            && dest.projection.is_empty()
        {
            callees.entry(dest.local).or_insert(&mir.func(*func).name);
        }
    }
    let temp = |id: Local, local: &LocalDecl| id != RETURN && local.name.is_none();
    let mut taken: HashSet<String> = HashSet::new();
    let mut names = vec![String::new(); body.locals.slots()];
    for (id, local) in body.locals.iter_with_ids() {
        if id == RETURN || temp(id, local) && !callees.contains_key(&id) {
            names[id.id] = format!("_{}", id.id);
            taken.insert(names[id.id].clone());
        }
    }
    let reserved = |n: &str| functions.contains(n) || n.starts_with("som_");
    for (id, local) in body.locals.iter_with_ids() {
        if let Some(source) = &local.name
            && names[id.id].is_empty()
        {
            names[id.id] = unique(source, "v", &mut taken, reserved);
        }
    }
    for (id, local) in body.locals.iter_with_ids() {
        if temp(id, local)
            && let Some(callee) = callees.get(&id)
        {
            // `som::data::Vector::x<f64>` is `x`.
            let callee = callee.split('<').next().unwrap_or(callee);
            let callee = callee.rsplit("::").next().unwrap_or(callee);
            names[id.id] = unique(&identifier(callee), "v", &mut taken, reserved);
        }
    }
    names
}

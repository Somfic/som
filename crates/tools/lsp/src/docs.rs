//! Definitions as they're declared in som, and their documentation: what hover, completion
//! and signature help show.

use som_ast::{Func, Id, Ident, Struct, Trait};
use som_driver::{Analysis, targets};
use som_resolve::{DefId, DefKind};
use som_typeck::{ArrayMethod, PointerMethod, Ty, TypeId};
use tower_lsp::lsp_types::{Documentation, MarkupContent, MarkupKind};

use crate::origin;
use crate::types::{struct_of, unref};

pub(crate) fn ty(a: &Analysis, t: TypeId) -> String {
    a.typed.display(t, &a.names)
}

pub(crate) fn describe(a: &Analysis, def: DefId) -> String {
    let d = a.names.def(def);
    match &d.kind {
        DefKind::Func(f) => signature(a, *f),
        DefKind::Struct(s) => struct_text(a, *s),
        DefKind::Primitive(_) => d.name.to_string(),
        DefKind::Param { func, index } if a.ast.funcs[*func].params[*index].out => {
            let param = &a.ast.funcs[*func].params[*index];
            match a.typed.try_local(def) {
                Some(t) => param_text(a, param, t, &[]),
                None => format!("out {}", d.name),
            }
        }
        DefKind::Variant { strukt, index } => {
            let def = &a.ast.structs[*strukt];
            format!("{}::{}", def.name, variant_text(a, *strukt, *index))
        }
        DefKind::Closure(_) => "closure".into(),
        DefKind::Param { .. }
        | DefKind::Local(_)
        | DefKind::Out(_)
        | DefKind::Binding(_)
        | DefKind::ClosureParam { .. } => {
            match a.typed.try_local(def) {
                Some(t) => format!("{}: {}", d.name, ty(a, t)),
                None => d.name.to_string(),
            }
        }
        DefKind::Const(c) => const_text(a, def, *c),
        DefKind::TypeParam { .. }
        | DefKind::TraitParam { .. }
        | DefKind::StructParam { .. }
        | DefKind::ImplParam { .. }
        | DefKind::AliasParam { .. } => {
            format!("type parameter {}", d.name)
        }
        DefKind::Trait(t) => trait_text(a, *t),
        DefKind::AssocType { .. } | DefKind::ImplType { .. } => format!("type {}", d.name),
        DefKind::Alias { ty } => {
            let params: Vec<&str> = (a.names.alias_params(def).iter())
                .map(|&p| &*a.names.def(p).name)
                .collect();
            format!(
                "type {}{} = {}",
                d.name,
                generics(&params),
                a.ast.type_text(*ty)
            )
        }
        DefKind::SelfType { impl_ } => match a.names.impls[*impl_].owner {
            Some(owner) => format!("Self = {}", a.names.def(owner).name),
            None => "Self".to_string(),
        },
    }
}

/// `@const CELLS: i32 = COLS * 20  // 200`: its value as written, and what that works out
/// to if it isn't a number already.
fn const_text(a: &Analysis, def: DefId, c: Id<som_ast::Const>) -> String {
    let name = &a.names.def(def).name;
    let mut text = match a.typed.try_local(def) {
        Some(t) => format!("@const {name}: {}", ty(a, t)),
        None => format!("@const {name}"),
    };
    let value = a.ast.consts[c].value;
    let span = a.ast.exprs.span(value);
    let source = span.source.content();
    if let Some(written) = source.get(span.start_offset..span.start_offset + span.length) {
        let written = written.split_whitespace().collect::<Vec<_>>().join(" ");
        text.push_str(&format!(" = {written}"));
        if let Some(n) = som_typeck::const_number(&a.ast, &a.names, value)
            && written != n.to_string()
        {
            text.push_str(&format!("  // {n}"));
        }
    }
    text
}

fn trait_text(a: &Analysis, t: Id<Trait>) -> String {
    let t = &a.ast.traits[t];
    let mut lines: Vec<String> = (t.types.iter())
        .map(|assoc| format!("    type {};", assoc.name))
        .collect();
    lines.extend(
        t.methods
            .iter()
            .map(|&m| format!("    {};", signature(a, m))),
    );
    format!("trait {} {{\n{}\n}}", t.name, lines.join("\n"))
}

fn generics<S: AsRef<str>>(params: &[S]) -> String {
    match params.is_empty() {
        true => String::new(),
        false => {
            let params: Vec<&str> = params.iter().map(AsRef::as_ref).collect();
            format!("<{}>", params.join(", "))
        }
    }
}

pub(crate) fn doc_of(a: &Analysis, def: DefId) -> Option<String> {
    match a.names.def(def).kind {
        DefKind::Func(f) => func_doc(a, f),
        DefKind::Struct(s) => struct_doc(a, s),
        DefKind::Const(c) => {
            let c = &a.ast.consts[c];
            let doc = with_availability(c.doc.clone(), &c.name);
            origin::with_link(doc, origin::of_name(&c.name))
        }
        _ => None,
    }
}

pub(crate) fn func_doc(a: &Analysis, f: Id<Func>) -> Option<String> {
    let doc = with_availability(a.ast.funcs[f].doc.clone(), &a.ast.funcs[f].name);
    origin::with_link(doc, origin::of_func(a, f))
}

pub(crate) fn struct_doc(a: &Analysis, s: Id<Struct>) -> Option<String> {
    let doc = with_availability(a.ast.structs[s].doc.clone(), &a.ast.structs[s].name);
    origin::with_link(doc, origin::of_name(&a.ast.structs[s].name))
}

/// `doc`, and for a C library name that's not on every system, which it's on.
pub(crate) fn with_availability(doc: Option<String>, name: &Ident) -> Option<String> {
    if !targets::in_libc(&name.span) {
        return doc;
    }
    let Some(note) = targets::availability_note(&name.name) else {
        return doc;
    };
    Some(match doc {
        Some(doc) => format!("{doc}\n\n{note}"),
        None => note,
    })
}

pub(crate) fn field_doc(
    a: &Analysis,
    object: Id<som_ast::Expr>,
    index: Option<usize>,
) -> Option<String> {
    let owner = struct_of(a, a.typed.expr(object))?;
    let DefKind::Struct(s) = a.names.def(owner).kind else {
        return None;
    };
    readable_fields(a, s).get(index?)?.0.doc.clone()
}

/// What `value.field` can read on a value of struct `s`: its fields; or for an enum, the
/// fields every variant has.
pub(crate) fn readable_fields(a: &Analysis, s: Id<Struct>) -> Vec<(&som_ast::Field, TypeId)> {
    let st = &a.ast.structs[s];
    let types = a.typed.fields(s);
    if !st.is_enum() {
        return st.fields.iter().zip(types.iter().copied()).collect();
    }
    let payload: Vec<&som_ast::Field> = st.payload().collect();
    (a.typed.common_fields(s).iter())
        .map(|c| (payload[c.payload[0]], types[c.payload[0]]))
        .collect()
}

pub(crate) fn markdown(doc: Option<String>) -> Option<Documentation> {
    doc.map(|value| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        })
    })
}

pub(crate) fn params_with(a: &Analysis, f: Id<Func>, subst: &[(DefId, TypeId)]) -> Vec<String> {
    a.ast.funcs[f]
        .params
        .iter()
        .zip(&a.typed.func(f).params)
        .map(|(p, &t)| param_text(a, p, t, subst))
        .collect()
}

/// `width: i32`, or `out remainder: i32` for a parameter whose type is `&mut i32`
/// underneath.
fn param_text(a: &Analysis, p: &som_ast::Param, t: TypeId, subst: &[(DefId, TypeId)]) -> String {
    let shown = |t| a.typed.table.display_with(t, &a.names, subst);
    match (p.out, a.typed.ty(t)) {
        (true, Ty::Ref { to, .. }) => format!("out {}: {}", p.name, shown(*to)),
        _ => format!("{}: {}", p.name, shown(t)),
    }
}

pub(crate) fn signature(a: &Analysis, f: Id<Func>) -> String {
    signature_with(a, f, &[])
}

/// `f`'s signature with type parameters shown as what `subst` says they are; those it
/// settles are left out of the `<…>`.
pub(crate) fn signature_with(a: &Analysis, f: Id<Func>, subst: &[(DefId, TypeId)]) -> String {
    let func = &a.ast.funcs[f];
    let sig = a.typed.func(f);
    let open: Vec<String> = (func.generics.iter())
        .zip(a.names.type_params(f))
        .filter(|(_, def)| !subst.iter().any(|(d, _)| d == *def))
        .map(|(p, _)| match p.ty {
            Some(ty) => format!("{}: {}", p.name.name, a.ast.type_text(ty)),
            None => p.name.name.to_string(),
        })
        .collect();
    let ret = if matches!(a.typed.ty(sig.ret), Ty::Unit) {
        String::new()
    } else if subst.is_empty() && a.typed.failures.sets.contains_key(&f) {
        // What it fails with, spelled out: `-> i64!(NotFound, ParseError)`.
        format!(" -> {}", som_typeck::failures::returns_text(&a.typed, &a.names, f))
    } else {
        format!(
            " -> {}",
            a.typed.table.display_with(sig.ret, &a.names, subst)
        )
    };
    let prefix = if func.body.is_none() { "extern " } else { "" };
    let mut params = params_with(a, f, subst);
    if func.variadic {
        params.push("...".into());
    }
    format!(
        "{prefix}fn {}{}({}){ret}",
        func.name,
        generics(&open),
        params.join(", ")
    )
}

pub(crate) fn struct_text(a: &Analysis, s: Id<Struct>) -> String {
    let def = &a.ast.structs[s];
    if def.is_enum() {
        let variants: Vec<String> = (0..def.variants().len())
            .map(|i| format!("    {},", variant_text(a, s, i)))
            .collect();
        return format!("enum {} {{\n{}\n}}", def.name, variants.join("\n"));
    }
    let fields: Vec<String> = def
        .fields
        .iter()
        .zip(a.typed.fields(s))
        .map(|(f, &t)| format!("    {}: {},", f.name, ty(a, t)))
        .collect();
    format!("struct {} {{\n{}\n}}", def.name, fields.join("\n"))
}

/// `Circle(f32)`, `Rect { w: f32, h: f32 }`.
pub(crate) fn variant_text(a: &Analysis, s: Id<Struct>, index: usize) -> String {
    let def = &a.ast.structs[s];
    let variant = &def.variants()[index];
    let start = def.payload_start(index);
    let types = &a.typed.fields(s)[start..start + variant.fields.len()];
    match variant.kind {
        som_ast::VariantKind::Unit => variant.name.to_string(),
        som_ast::VariantKind::Tuple => {
            let types: Vec<String> = types.iter().map(|&t| ty(a, t)).collect();
            format!("{}({})", variant.name, types.join(", "))
        }
        som_ast::VariantKind::Named => {
            let fields: Vec<String> = (variant.fields.iter().zip(types))
                .map(|(f, &t)| format!("{}: {}", f.name, ty(a, t)))
                .collect();
            format!("{} {{ {} }}", variant.name, fields.join(", "))
        }
    }
}

/// An array, slice or text, whose methods are built in.
#[derive(Clone, Copy)]
pub(crate) struct Sequence {
    pub element: TypeId,
    /// How many elements, if that's fixed.
    pub len: Option<u64>,
    pub is_text: bool,
}

pub(crate) fn array_of(a: &Analysis, t: TypeId) -> Option<Sequence> {
    let (element, len, is_text) = match a.typed.ty(unref(a, t)) {
        Ty::Array { element, len } => {
            let len = match a.typed.ty(*len) {
                Ty::Value(som_typeck::Value::Int(n)) => Some(*n as u64),
                // `N` in a generic function: only known in each copy of it.
                _ => None,
            };
            (*element, len, false)
        }
        Ty::Slice(element) => (*element, None, false),
        Ty::Str => (a.typed.table.u8, None, true),
        _ => return None,
    };
    Some(Sequence {
        element,
        len,
        is_text,
    })
}

/// A built-in method's parameters, result and documentation. In the types, `T` stands for
/// the element type.
struct BuiltinMethod {
    params: &'static [(&'static str, &'static str)],
    returns: &'static str,
    doc: &'static str,
}

const ELEMENT: char = 'T';
const INDEX: (&str, &str) = ("index", "any integer");
const RANGE: (&str, &str) = ("range", "Range<any integer>");

fn array_method(method: ArrayMethod) -> BuiltinMethod {
    let (params, returns, doc): (&'static [_], _, _) = match method {
        ArrayMethod::Len => (
            &[],
            "any integer",
            "The number of elements. It works as any integer type.",
        ),
        ArrayMethod::Get => (
            &[INDEX],
            "T?",
            "A copy of the element at `index`, or `none` if `index` is out of bounds.",
        ),
        ArrayMethod::GetRef => (
            &[INDEX],
            "&T?",
            "A reference to the element at `index`, or `none` if `index` is out of bounds.",
        ),
        ArrayMethod::GetMut => (
            &[INDEX],
            "&mut T?",
            "A mutable reference to the element at `index`, or `none` if `index` is out of \
             bounds.",
        ),
        ArrayMethod::Slice => (
            &[RANGE],
            "&[T]?",
            "A view of the elements in `range`, or `none` if the range isn't within them. An \
             end left out, as in `1..`, is their own.",
        ),
        ArrayMethod::SliceMut => (
            &[RANGE],
            "&mut [T]?",
            "A view of the elements in `range` that can change them, or `none` if the range \
             isn't within them. An end left out, as in `1..`, is their own.",
        ),
        ArrayMethod::Bytes => (
            &[],
            "[u8]",
            "The text's bytes, as a slice: the same view, so it costs nothing.",
        ),
        ArrayMethod::Set => (
            &[INDEX, ("value", "T")],
            "bool",
            "Stores `value` at `index`, and returns whether `index` was in bounds. If it \
             wasn't, nothing changes.",
        ),
    };
    BuiltinMethod {
        params,
        returns,
        doc,
    }
}

fn pointer_method(method: PointerMethod) -> BuiltinMethod {
    let (params, returns, doc): (&'static [_], _, _) = match method {
        PointerMethod::Resize => (
            &[("count", "usize")],
            "",
            "Makes it room for `count` values, keeping the ones there (as many as fit). A null \
             one starts empty; copies of the old pointer aren't valid any more.",
        ),
        PointerMethod::Free => (&[], "", "Gives the memory back."),
        PointerMethod::Take => (
            &[("index", "usize")],
            "T",
            "The value at `index`, copied or moved out. Nothing checks it's there.",
        ),
        PointerMethod::Put => (
            &[("index", "usize"), ("value", "T")],
            "",
            "Stores `value` at `index`, without reading or freeing what was there.",
        ),
        PointerMethod::At => (
            &[("index", "usize")],
            "&T",
            "A reference to the value at `index`.",
        ),
        PointerMethod::AtMut => (
            &[("index", "usize")],
            "&mut T",
            "A reference to the value at `index`, to change it.",
        ),
        PointerMethod::View => (
            &[("len", "usize")],
            "&[T]",
            "The `len` values starting here, as a slice.",
        ),
        PointerMethod::ViewMut => (
            &[("len", "usize")],
            "&mut [T]",
            "The `len` values starting here, as a slice that can change them.",
        ),
        PointerMethod::IsNull => (&[], "bool", "Whether it points to nothing."),
    };
    BuiltinMethod {
        params,
        returns,
        doc,
    }
}

/// A built-in method as the editor shows it, for one element type.
pub(crate) struct MethodDoc {
    pub name: &'static str,
    pub signature: String,
    pub params: Vec<String>,
    pub param_names: Vec<&'static str>,
    pub doc: String,
}

impl BuiltinMethod {
    fn for_element(&self, name: &'static str, element: &str) -> MethodDoc {
        let fill = |s: &str| s.replace(ELEMENT, element);
        let params: Vec<String> = (self.params.iter())
            .map(|(p, t)| format!("{p}: {}", fill(t)))
            .collect();
        let returns = match self.returns {
            "" => String::new(),
            returns => format!(" -> {}", fill(returns)),
        };
        MethodDoc {
            name,
            signature: format!("fn {name}({}){returns}", params.join(", ")),
            params,
            param_names: self.params.iter().map(|(p, _)| *p).collect(),
            doc: self.doc.to_string(),
        }
    }
}

pub(crate) fn array_method_doc(a: &Analysis, method: ArrayMethod, sequence: Sequence) -> MethodDoc {
    let mut spec = array_method(method);
    if sequence.is_text && method == ArrayMethod::Slice {
        spec.returns = "str?";
    }
    let mut doc = spec.for_element(method.name(), &ty(a, sequence.element));
    // `len` is a property: `items.len`.
    if method == ArrayMethod::Len {
        doc.signature = format!("{}: {}", method.name(), spec.returns);
        if let Some(len) = sequence.len {
            doc.doc = format!("The number of elements: {len}. It works as any integer type.");
        }
    }
    doc
}

pub(crate) fn pointer_method_doc(method: PointerMethod, element: &str) -> MethodDoc {
    pointer_method(method).for_element(method.name(), element)
}

//! Names that GLSL takes: only letters, digits and single `_`s, and none of its own.

/// `text` as a GLSL identifier: `som::data::Vector<4, f32>` is `som_data_Vector_4_f32`.
pub(crate) fn ident(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        // `__` is GLSL's, and a leading `_` reads as a temporary.
        if c == '_' && (out.is_empty() || out.ends_with('_')) {
            continue;
        }
        out.push(c);
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'x');
    }
    out
}

/// A struct's member named `name`: as it is, unless GLSL has the name.
pub(crate) fn member(name: &str) -> String {
    let name = ident(name);
    match is_reserved(&name) {
        true => format!("{name}_"),
        false => name,
    }
}

/// What a program can't name something in GLSL: its keywords, words it keeps for later, its
/// types and built-in variables, and what the code around a shader declares.
pub(crate) fn is_reserved(name: &str) -> bool {
    name.starts_with("gl_")
        || name.contains("__")
        || KEYWORDS.contains(&name)
        || [
            "vec", "ivec", "uvec", "bvec", "dvec", "mat", "dmat", "sampler", "isampler",
            "usampler", "image", "iimage", "uimage",
        ]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| !rest.is_empty() && rest.starts_with(|c: char| c.is_ascii_digit()))
        })
}

const KEYWORDS: &[&str] = &[
    // Keywords.
    "attribute", "const", "uniform", "varying", "layout", "centroid", "flat", "smooth",
    "noperspective", "patch", "sample", "break", "continue", "do", "for", "while", "switch",
    "case", "default", "if", "else", "subroutine", "in", "out", "inout", "float", "double",
    "int", "void", "bool", "true", "false", "invariant", "precise", "discard", "return",
    "lowp", "mediump", "highp", "precision", "struct", "uint", "buffer", "shared", "coherent",
    "volatile", "restrict", "readonly", "writeonly", "atomic_uint",
    // Kept for later.
    "common", "partition", "active", "asm", "class", "union", "enum", "typedef", "template",
    "this", "resource", "goto", "inline", "noinline", "public", "static", "extern", "external",
    "interface", "long", "short", "half", "fixed", "unsigned", "superp", "input", "output",
    "hvec2", "hvec3", "hvec4", "fvec2", "fvec3", "fvec4", "filter", "sizeof", "cast",
    "namespace", "using", "row_major", "column_major",
    // Names raylib's shaders and the code around a shader use.
    "main", "fragTexCoord", "fragColor", "finalColor", "vertexPosition", "vertexTexCoord",
    "vertexColor", "vertexNormal", "mvp", "texture0", "colDiffuse",
];

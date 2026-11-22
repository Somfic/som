//! Names that WGSL takes: only letters, digits and single `_`s, and none of its own.

/// `text` as a WGSL identifier: `som::data::Vector<4, f32>` is `som_data_Vector_4_f32`.
pub(crate) fn ident(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        // `__` is WGSL's, and a leading `_` reads as a temporary.
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

/// A struct's member named `name`: as it is, unless WGSL has the name.
pub(crate) fn member(name: &str) -> String {
    let name = ident(name);
    match is_reserved(&name) {
        true => format!("{name}_"),
        false => name,
    }
}

/// What a program can't name something in WGSL: its keywords, the words it keeps for later,
/// its types, and its built-in functions (which a name would hide).
pub(crate) fn is_reserved(name: &str) -> bool {
    name.starts_with("__")
        || KEYWORDS.contains(&name)
        || RESERVED.contains(&name)
        || TYPES.contains(&name)
        || FUNCTIONS.contains(&name)
        || ["vec", "mat", "texture_"].iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()) || *prefix == "texture_")
        })
}

const KEYWORDS: &[&str] = &[
    "alias", "break", "case", "const", "const_assert", "continue", "continuing", "default",
    "diagnostic", "discard", "else", "enable", "false", "fn", "for", "if", "let", "loop",
    "override", "requires", "return", "struct", "switch", "true", "var", "while",
];

const RESERVED: &[&str] = &[
    "NULL", "Self", "abstract", "active", "alignas", "alignof", "as", "asm", "asm_fragment",
    "async", "attribute", "auto", "await", "become", "binding_array", "cast", "catch", "class",
    "co_await", "co_return", "co_yield", "coherent", "column_major", "common", "compile",
    "compile_fragment", "concept", "const_cast", "consteval", "constexpr", "constinit",
    "crate", "debugger", "decltype", "delete", "demote", "demote_to_helper", "do",
    "dynamic_cast", "enum", "explicit", "export", "extends", "extern", "external",
    "fallthrough", "filter", "final", "finally", "friend", "from", "fxgroup", "get", "goto",
    "groupshared", "highp", "impl", "implements", "import", "inline", "instanceof",
    "interface", "layout", "lowp", "macro", "macro_rules", "match", "mediump", "meta", "mod",
    "module", "move", "mut", "mutable", "namespace", "new", "nil", "noexcept", "noinline",
    "nointerpolation", "noperspective", "null", "nullptr", "of", "operator", "package",
    "packoffset", "partition", "pass", "patch", "pixelfragment", "precise", "precision",
    "premerge", "priv", "protected", "pub", "public", "readonly", "ref", "regardless",
    "register", "reinterpret_cast", "require", "resource", "restrict", "self", "set",
    "shared", "sizeof", "smooth", "snorm", "static", "static_assert", "static_cast", "std",
    "subroutine", "super", "target", "template", "this", "thread_local", "throw", "trait",
    "try", "type", "typedef", "typeid", "typename", "typeof", "union", "unless", "unorm",
    "unsafe", "unsized", "use", "using", "varying", "virtual", "volatile", "wgsl", "where",
    "with", "writeonly", "yield",
];

const TYPES: &[&str] = &[
    "bool", "f16", "f32", "i32", "u32", "array", "atomic", "ptr", "sampler",
    "sampler_comparison", "function", "private", "workgroup", "uniform", "storage", "read",
    "write", "read_write",
];

/// WGSL's built-in functions.
const FUNCTIONS: &[&str] = &[
    "bitcast", "all", "any", "select", "arrayLength", "abs", "acos", "acosh", "asin", "asinh",
    "atan", "atanh", "atan2", "ceil", "clamp", "cos", "cosh", "countLeadingZeros",
    "countOneBits", "countTrailingZeros", "cross", "degrees", "determinant", "distance",
    "dot", "exp", "exp2", "extractBits", "faceForward", "firstLeadingBit",
    "firstTrailingBit", "floor", "fma", "fract", "frexp", "insertBits", "inverseSqrt",
    "ldexp", "length", "log", "log2", "max", "min", "mix", "modf", "normalize", "pow",
    "quantizeToF16", "radians", "reflect", "refract", "reverseBits", "round", "saturate",
    "sign", "sin", "sinh", "smoothstep", "sqrt", "step", "tan", "tanh", "transpose", "trunc",
    "dpdx", "dpdxCoarse", "dpdxFine", "dpdy", "dpdyCoarse", "dpdyFine", "fwidth",
    "textureDimensions", "textureGather", "textureLoad", "textureNumLayers",
    "textureNumLevels", "textureSample", "textureSampleBias", "textureSampleCompare",
    "textureSampleGrad", "textureSampleLevel", "textureStore", "atomicLoad", "atomicStore",
    "atomicAdd", "pack4x8snorm", "pack4x8unorm", "unpack4x8snorm", "unpack4x8unorm",
    "storageBarrier", "workgroupBarrier",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_names() {
        for name in ["loop", "vec4", "mat3x3", "texture_2d", "sin", "f32", "mod", "__x"] {
            assert!(is_reserved(name), "{name}");
        }
        for name in ["time", "tint", "texture", "vector", "matter", "colour"] {
            assert!(!is_reserved(name), "{name}");
        }
        assert_eq!(member("loop"), "loop_");
        assert_eq!(member("camera.pos"), "camera_pos");
    }
}

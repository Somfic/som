//! The names GLSL takes, for the shader translator and for the uniforms lowering names.

/// What a program can't name something in GLSL: its keywords, words it keeps for later, its
/// types, built-in variables and functions, and what the code around a shader declares.
/// Here, not in the shader translator, since lowering names the uniforms the host sets and
/// they're named alike.
pub fn glsl_reserved(name: &str) -> bool {
    name.starts_with("gl_")
        || name.contains("__")
        || KEYWORDS.contains(&name)
        || GLSL_FUNCTIONS.contains(&name)
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

/// GLSL 3.30's built-in functions: a uniform or variable by one of these names hides it.
const GLSL_FUNCTIONS: &[&str] = &[
    "radians", "degrees", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
    "asinh", "acosh", "atanh", "pow", "exp", "log", "exp2", "log2", "sqrt", "inversesqrt",
    "abs", "sign", "floor", "trunc", "round", "roundEven", "ceil", "fract", "mod", "modf",
    "min", "max", "clamp", "mix", "step", "smoothstep", "isnan", "isinf", "floatBitsToInt",
    "floatBitsToUint", "intBitsToFloat", "uintBitsToFloat", "length", "distance", "dot",
    "cross", "normalize", "faceforward", "reflect", "refract", "matrixCompMult",
    "outerProduct", "transpose", "determinant", "inverse", "lessThan", "lessThanEqual",
    "greaterThan", "greaterThanEqual", "equal", "notEqual", "any", "all", "not",
    "textureSize", "texture", "textureProj", "textureLod", "textureOffset", "texelFetch",
    "texelFetchOffset", "textureProjOffset", "textureLodOffset", "textureProjLod",
    "textureProjLodOffset", "textureGrad", "textureGradOffset", "textureProjGrad",
    "textureProjGradOffset", "dFdx", "dFdy", "fwidth", "noise1", "noise2", "noise3", "noise4",
    "EmitVertex", "EndPrimitive",
];

/// A struct member named `name` as GLSL has it: with a `_` after it if GLSL has the name.
/// What the host sets a uniform in a struct by, `material.sample_`, too.
pub fn glsl_member(name: &str) -> String {
    match glsl_reserved(name) {
        true => format!("{name}_"),
        false => name.to_string(),
    }
}

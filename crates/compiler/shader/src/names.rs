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

/// What a program can't name something in GLSL: `som_lang::glsl_reserved`.
pub(crate) fn is_reserved(name: &str) -> bool {
    som_lang::glsl_reserved(name)
}

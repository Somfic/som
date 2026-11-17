//! What the text around the cursor says, for when the analysis is of text that's since
//! changed or doesn't parse yet.

use std::collections::HashSet;

pub(crate) fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Where the word the cursor at `offset` is in, or right after, starts.
pub(crate) fn word_start(text: &str, offset: usize) -> usize {
    text[..offset].trim_end_matches(is_word).len()
}

/// The `.` right before the word being typed, as in `alloc(10).si`: not a range's `..`.
pub(crate) fn dot_before_word(text: &str, offset: usize) -> Option<usize> {
    let start = word_start(text, offset);
    let before = &text[..start];
    (before.ends_with('.') && !before.ends_with("..")).then(|| start - 1)
}

/// Where the struct literal the cursor at `offset` is in ends: past its `}` if only spaces
/// come before it, otherwise at `offset`.
pub(crate) fn literal_end(text: &str, offset: usize) -> usize {
    let rest = &text[offset..];
    let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    match rest[spaces..].starts_with('}') {
        true => offset + spaces + 1,
        false => offset,
    }
}

/// When the cursor is where a field's name goes in a struct literal, `Color { r: 1, |`:
/// the struct's path, and the fields already written in it, before or after the cursor.
pub(crate) fn struct_literal(text: &str, offset: usize) -> Option<(&str, HashSet<&str>)> {
    let open = enclosing_brace(text.get(..offset)?)?;

    let before = text[..open].trim_end();
    let start = before
        .rfind(|c: char| !(is_word(c) || c == ':'))
        .map_or(0, |i| i + 1);
    let path = before[start..].trim_start_matches(':');
    let first = path.chars().next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    let keyword = before[..start]
        .trim_end()
        .rsplit(|c: char| !is_word(c))
        .next()
        .unwrap_or_default();
    if matches!(keyword, "struct" | "impl" | "fn") || before[..start].trim_end().ends_with("->") {
        return None;
    }

    let mut written = HashSet::new();
    for field in literal_fields(text, open) {
        if field.contains(&offset) || field.end == offset {
            // After `name:` comes a value, not a field.
            if field_name(&text[field.start..offset]).is_some() {
                return None;
            }
        } else if let Some(name) = field_name(&text[field]) {
            written.insert(name);
        }
    }
    Some((path, written))
}

/// The `{` that the end of `before` is inside of, looking back past nested brackets.
fn enclosing_brace(before: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in before.char_indices().rev() {
        match c {
            '}' | ')' | ']' => depth += 1,
            '{' | '(' | '[' if depth > 0 => depth -= 1,
            '{' => return Some(i),
            '(' | '[' | ';' => return None,
            _ => {}
        }
    }
    None
}

/// The fields of the literal opened at `open`, split at the commas outside nested brackets,
/// up to its `}`.
fn literal_fields(text: &str, open: usize) -> Vec<std::ops::Range<usize>> {
    let mut fields = Vec::new();
    let (mut depth, mut from, mut end) = (0, open + 1, text.len());
    for (i, c) in text[open + 1..].char_indices() {
        let i = i + open + 1;
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' if depth == 0 => {
                end = i;
                break;
            }
            '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(from..i);
                from = i + 1;
            }
            _ => {}
        }
    }
    fields.push(from..end);
    fields
}

/// The name in a struct literal's `name: value`.
fn field_name(field: &str) -> Option<&str> {
    let field = field.trim_start();
    let len = field.find(|c: char| !is_word(c)).unwrap_or(field.len());
    let rest = field[len..].trim_start();
    (len > 0 && rest.starts_with(':') && !rest.starts_with("::")).then(|| &field[..len])
}

/// Whether the cursor is where `extern`'s library name goes: in its string, `extern "ray`
/// (`Some(true)`), or before the string is opened, `extern ray` (`Some(false)`).
pub(crate) fn extern_library(text: &str, offset: usize) -> Option<bool> {
    let after_extern = |before: &str| {
        before
            .strip_suffix("extern")
            .is_some_and(|rest| !rest.ends_with(is_word))
    };
    let before = text.get(..offset)?;
    if let Some(quote) = before.rfind('"')
        && !before[quote + 1..].contains('\n')
        && after_extern(before[..quote].trim_end())
    {
        return Some(true);
    }
    let rest = before.trim_end_matches(is_word);
    let spaced = rest.ends_with(char::is_whitespace);
    (spaced && after_extern(rest.trim_end())).then_some(false)
}

/// In `use som::da`: the path before the part being typed, `som::`; `""` right after `use`.
pub(crate) fn use_path(text: &str, offset: usize) -> Option<&str> {
    let before = text.get(..offset)?;
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let path = line.trim_start().strip_prefix("use ")?.trim_start();
    if !path.chars().all(|c| is_word(c) || c == ':') {
        return None;
    }
    Some(path.trim_end_matches(is_word))
}

/// The path right before the word being typed: `raylib` in `raylib::ini`, `som::io::File`
/// in `som::io::File::op`.
pub(crate) fn qualifier(text: &str, offset: usize) -> Option<&str> {
    let before = text.get(..offset)?;
    let word = before.trim_end_matches(is_word);
    let path = word.strip_suffix("::")?;
    let start = path
        .rfind(|c: char| !(is_word(c) || c == ':'))
        .map_or(0, |i| i + 1);
    Some(&path[start..]).filter(|p| !p.is_empty() && !p.starts_with(':'))
}

/// What can be written at the cursor in an `impl` header or a generic's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TypeSlot {
    /// `impl ` or `impl<T> `: a trait, or the type the methods are for.
    TraitOrType,
    /// `impl Add for `
    Type,
    /// `fn f<T: ` or `T: Eq + `
    Trait,
}

pub(crate) fn type_slot(text: &str, offset: usize) -> Option<TypeSlot> {
    let before = text[..offset].trim_end_matches(is_word);
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let trimmed = line.trim_start();
    let Some(rest) = trimmed
        .strip_prefix("impl")
        .filter(|r| !r.starts_with(is_word))
    else {
        return bound_slot(line);
    };
    let rest = match rest.trim_start().strip_prefix('<') {
        Some(generics) => match generics.find('>') {
            Some(end) => &generics[end + 1..],
            None => return bound_slot(line),
        },
        None => rest,
    };
    match rest.trim() {
        "" => Some(TypeSlot::TraitOrType),
        rest if rest.ends_with(" for") || rest == "for" => Some(TypeSlot::Type),
        _ => None,
    }
}

/// In `fn f<T: ` or `struct S<K: Hash + `: after a `:` or `+` inside the `<…>` of a
/// declaration's generics.
fn bound_slot(line: &str) -> Option<TypeSlot> {
    let trimmed = line.trim_end();
    if !(trimmed.ends_with(':') && !trimmed.ends_with("::") || trimmed.ends_with('+')) {
        return None;
    }
    let mut depth = 0;
    for (i, c) in trimmed.char_indices().rev() {
        match c {
            '>' => depth += 1,
            '<' if depth > 0 => depth -= 1,
            '<' => {
                let head = trimmed[..i].trim_end();
                let keyword = head.trim_end_matches(is_word).trim_end();
                let declares = ["fn", "struct", "enum", "trait"]
                    .iter()
                    .any(|k| keyword.ends_with(k))
                    || head.ends_with("impl");
                return declares.then_some(TypeSlot::Trait);
            }
            '(' | ')' | '{' | '}' | ';' | '=' => return None,
            _ => {}
        }
    }
    None
}

/// Whether a type goes at `offset`: after `->`, or after the `:` of a `let`, a parameter or
/// a struct's field definition, but not a struct literal's field value.
pub(crate) fn type_position(text: &str, offset: usize) -> bool {
    // Past the word being typed and its qualifier: `raylib::Col`.
    let mut before = text[..offset].trim_end_matches(is_word);
    while let Some(rest) = before.strip_suffix("::") {
        before = rest.trim_end_matches(is_word);
    }
    let before = before.trim_end().trim_end_matches('&').trim_end();
    let before = before
        .strip_suffix("mut")
        .filter(|b| !b.ends_with(is_word))
        .unwrap_or(before)
        .trim_end();
    if before.ends_with("->") {
        return true;
    }
    let Some(before) = before.strip_suffix(':').filter(|b| !b.ends_with(':')) else {
        return false;
    };
    let named = before.trim_end().trim_end_matches(is_word).trim_end();
    let keyword = named
        .rsplit(|c: char| !is_word(c))
        .next()
        .unwrap_or_default();
    if matches!(keyword, "let" | "mut") {
        return true;
    }
    // `fn f(a: …` or `struct S { a: …`: what the enclosing bracket belongs to.
    let mut depth = 0;
    for (i, c) in named.char_indices().rev() {
        match c {
            ')' | '}' | ']' => depth += 1,
            '(' | '{' | '[' if depth > 0 => depth -= 1,
            '(' | '{' => {
                let name = named[..i].trim_end();
                let name = match name.strip_suffix('>') {
                    Some(generic) => &generic[..generic.rfind('<').unwrap_or(0)],
                    None => name,
                };
                let keyword = name.trim_end_matches(is_word).trim_end();
                return keyword.ends_with(if c == '(' { "fn" } else { "struct" });
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_ends_past_its_brace() {
        assert_eq!(literal_end("File { }", 7), 8);
        assert_eq!(literal_end("File {}", 6), 7);
        assert_eq!(literal_end("File { ", 7), 7);
        assert_eq!(literal_end("File { x }", 7), 7);
    }

    #[test]
    fn library_names_complete_in_extern_strings() {
        let at_end = |text: &str| extern_library(text, text.len());
        assert_eq!(at_end("extern \""), Some(true));
        assert_eq!(at_end("use std;\nextern \"ray"), Some(true));
        assert_eq!(at_end("extern\"gc"), Some(true));
        assert_eq!(at_end("extern "), Some(false));
        assert_eq!(at_end("use std;\nextern ray"), Some(false));
        assert_eq!(at_end("extern"), None);
        assert_eq!(at_end("extern \"raylib\";\n"), None);
        assert_eq!(at_end("extern \"raylib\" from \"ray"), None);
        assert_eq!(at_end("my_extern \""), None);
        assert_eq!(at_end("my_extern ray"), None);
        assert_eq!(at_end("let s = \"ray"), None);
    }

    #[test]
    fn struct_literals_and_their_written_fields() {
        fn at_end(text: &str) -> Option<(&str, Vec<&str>)> {
            struct_literal(text, text.len()).map(|(path, written)| {
                let mut written: Vec<&str> = written.into_iter().collect();
                written.sort();
                (path, written)
            })
        }
        assert_eq!(
            at_end("let c = raylib::Color { "),
            Some(("raylib::Color", vec![]))
        );
        assert_eq!(
            at_end("Color { r: 1, g: f(2, 3), "),
            Some(("Color", vec!["g", "r"]))
        );
        assert_eq!(
            at_end("Color { r: Point { x: 1 }, b"),
            Some(("Color", vec!["r"]))
        );
        assert_eq!(at_end("Color { r: "), None, "a value goes here");
        assert_eq!(at_end("Color { r: raylib::"), None);
        assert_eq!(at_end("struct Color { "), None);
        assert_eq!(at_end("fn main() { "), None);
        assert_eq!(at_end("f("), None);

        let text = "Color { r: 1, , b: 3 }";
        let (_, written) = struct_literal(text, text.find(", ,").unwrap() + 2).unwrap();
        assert_eq!(written, HashSet::from(["r", "b"]));
    }

    #[test]
    fn types_go_after_arrows_and_annotations_not_in_literals() {
        for (text, is_type) in [
            ("let c: raylib::Col", true),
            ("let mut c: &Col", true),
            ("fn f(a: i32, b: Col", true),
            ("fn f<T>(a: Col", true),
            ("fn f() -> Col", true),
            ("struct Rect { color: Col", true),
            ("let r = Rect { color: Col", false),
            ("let c = raylib::Col", false),
            ("    Col", false),
            ("f(Col", false),
        ] {
            assert_eq!(type_position(text, text.len()), is_type, "{text}");
        }
    }

    #[test]
    fn impl_headers_and_bounds_take_traits_and_types() {
        for (text, slot) in [
            ("impl ", Some(TypeSlot::TraitOrType)),
            ("impl Ad", Some(TypeSlot::TraitOrType)),
            ("impl<T> Ite", Some(TypeSlot::TraitOrType)),
            ("impl Add for ", Some(TypeSlot::Type)),
            ("impl<T> From<[T; 3]> for Li", Some(TypeSlot::Type)),
            ("fn sum<T: ", Some(TypeSlot::Trait)),
            ("fn sum<T: Add + Co", Some(TypeSlot::Trait)),
            ("impl<T: Eq", Some(TypeSlot::Trait)),
            ("struct Map<K: Ha", Some(TypeSlot::Trait)),
            ("let x: ", None),
            ("    impl_thing", None),
            ("impl Add for V { fn ", None),
            ("let y = a < b + ", None),
        ] {
            assert_eq!(type_slot(text, text.len()), slot, "{text}");
        }
    }

    #[test]
    fn use_paths_are_what_comes_before_the_word() {
        let at_end = |text: &'static str| use_path(text, text.len());
        assert_eq!(at_end("use "), Some(""));
        assert_eq!(at_end("use som::"), Some("som::"));
        assert_eq!(at_end("use som::da"), Some("som::"));
        assert_eq!(at_end("    use ray"), Some(""));
        assert_eq!(at_end("let use_it = "), None);
        assert_eq!(at_end("use som; let x = "), None);
    }

    #[test]
    fn qualifier_is_the_path_before_the_word() {
        let at_end = |text: &'static str| qualifier(text, text.len());
        assert_eq!(at_end("    raylib::ini"), Some("raylib"));
        assert_eq!(at_end("(raylib::"), Some("raylib"));
        assert_eq!(at_end("= som::io::File::op"), Some("som::io::File"));
        assert_eq!(at_end("(io::File::"), Some("io::File"));
        assert_eq!(at_end("    rayl"), None);
        assert_eq!(at_end("::x"), None);
    }
}

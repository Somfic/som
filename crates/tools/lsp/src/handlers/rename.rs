//! Finding a symbol's references, highlighting them, and renaming it.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use som_base::Source;
use som_driver::Analysis;
use som_lexer::{TokenKind, lex};
use som_resolve::DefKind;
use tower_lsp::lsp_types::{
    DocumentHighlight, DocumentHighlightKind, Location, TextEdit, Url, WorkspaceEdit,
};

use super::Cursor;
use crate::convert;
use crate::lookup::covers;
use crate::references::{self, Occurrence, Symbol};

/// The symbol named at a place, and every place it, or what shares its name, is named.
pub(crate) struct Uses {
    pub at: Occurrence,
    pub matching: Vec<Occurrence>,
    pub all: Vec<Occurrence>,
}

pub(crate) fn uses_at(a: &Analysis, file: &str, offset: usize) -> Option<Uses> {
    let all = references::occurrences(a);
    let symbol = references::symbol_at(&all, file, offset)?;
    let at = (all.iter())
        .find(|o| o.symbol == symbol && covers(&o.span, file, offset))?
        .clone();
    let family = references::family(a, symbol);
    let mut seen = HashSet::new();
    let matching = (all.iter())
        .filter(|o| family.contains(&o.symbol))
        .filter(|o| seen.insert((o.span.source.identifier().to_string(), o.span.start_offset)))
        .cloned()
        .collect();
    Some(Uses { at, matching, all })
}

pub(crate) fn references(
    a: &Analysis,
    cursor: &Cursor,
    declarations: bool,
) -> Option<Vec<Location>> {
    let uses = uses_at(a, &cursor.file, cursor.offset)?;
    Some(
        (uses.matching.iter())
            .filter(|o| declarations || !o.definition)
            .filter_map(|o| convert::location(&o.span))
            .collect(),
    )
}

/// Every other place in this file the symbol under the cursor is named.
pub(crate) fn highlights(a: &Analysis, cursor: &Cursor) -> Option<Vec<DocumentHighlight>> {
    let uses = uses_at(a, &cursor.file, cursor.offset)?;
    Some(
        (uses.matching.iter())
            .filter(|o| o.span.source.identifier() == cursor.file)
            .map(|o| DocumentHighlight {
                range: convert::span_to_range(&o.span),
                kind: Some(if o.definition {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                }),
            })
            .collect(),
    )
}

/// Why the symbol can't be renamed, if it can't: it's C's, the language's own, or the
/// standard library's when renamed from outside it.
pub(crate) fn renamable(a: &Analysis, uses: &Uses) -> Result<(), String> {
    if let Symbol::Def(def) = uses.at.symbol {
        match a.names.def(def).kind {
            DefKind::Primitive(_) => return Err("a built-in type can't be renamed".into()),
            DefKind::SelfType { .. } => {
                return Err("`Self` can't be renamed: rename the type itself".into());
            }
            DefKind::Func(f) => {
                let func = &a.ast.funcs[f];
                if func.body.is_none() && !func.in_trait {
                    return Err(format!(
                        "`{}` is a C function: its name is C's",
                        func.name.name
                    ));
                }
            }
            _ => {}
        }
    }
    if !uses.matching.iter().any(|o| o.definition) {
        return Err("this isn't defined in your code, so it can't be renamed".into());
    }
    let in_std =
        |o: &Occurrence| som_driver::stdlib::contains(Path::new(o.span.source.identifier()));
    if !in_std(&uses.at) && uses.matching.iter().any(|o| o.definition && in_std(o)) {
        return Err(format!(
            "`{}` is the standard library's, so it can't be renamed",
            references::name_of(a, uses.at.symbol)
        ));
    }
    Ok(())
}

/// Why `name` can't be a name, if it can't. Lexed, so it agrees with the lexer on what's a
/// keyword, a built-in type or a number.
pub(crate) fn valid_name(name: &str) -> Result<(), String> {
    let tokens = lex(Arc::new(Source::from_raw(name)));
    let words: Vec<_> = (tokens.iter())
        .filter(|t| !matches!(t.kind, TokenKind::Whitespace | TokenKind::Eof))
        .collect();
    match words.as_slice() {
        [word] if word.kind == TokenKind::Ident => Ok(()),
        [word] if word.text.chars().all(|c| c.is_alphanumeric() || c == '_') => {
            Err(format!("`{name}` is a keyword, so it can't be a name"))
        }
        _ => Err(format!(
            "`{name}` isn't a name: use letters, digits and `_`"
        )),
    }
}

/// The edits renaming every one of `uses` to `new`. In `Point { x }` the field and the
/// variable are named once: renaming one writes both, `Point { y: x }` or `Point { x: y }`.
pub(crate) fn rename_edit(uses: &Uses, new: &str) -> WorkspaceEdit {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for o in &uses.matching {
        let Ok(uri) = Url::from_file_path(o.span.source.identifier()) else {
            continue;
        };
        let old = o.span.get_text();
        let text = if o.shorthand {
            format!("{new}: {old}")
        } else if (uses.all.iter()).any(|f| {
            f.shorthand
                && f.symbol != o.symbol
                && f.span.start_offset == o.span.start_offset
                && f.span.source.identifier() == o.span.source.identifier()
        }) {
            format!("{old}: {new}")
        } else {
            new.to_string()
        };
        changes.entry(uri).or_default().push(TextEdit {
            range: convert::span_to_range(&o.span),
            new_text: text,
        });
    }
    WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{POINTS, analyzed};

    /// Where `uses` are, as the text from each one's start to the end of its line.
    fn places(source: &str, uses: &[Occurrence]) -> Vec<String> {
        let mut places: Vec<_> = uses.iter().map(|o| o.span.start_offset).collect();
        places.sort();
        (places.into_iter())
            .map(|at| source[at..].lines().next().unwrap().to_string())
            .collect()
    }

    /// The source with `edit` applied, for one file.
    fn applied(source: &str, edit: &WorkspaceEdit) -> String {
        let mut edits: Vec<&TextEdit> = edit.changes.as_ref().unwrap().values().flatten().collect();
        edits.sort_by_key(|e| std::cmp::Reverse((e.range.start.line, e.range.start.character)));
        let mut text = source.to_string();
        for e in edits {
            let start = convert::position_to_offset(&text, &e.range.start).unwrap();
            let end = convert::position_to_offset(&text, &e.range.end).unwrap();
            text.replace_range(start..end, &e.new_text);
        }
        text
    }

    #[test]
    fn a_field_is_found_where_its_defined_given_and_read() {
        let (_dir, file, a) = analyzed(POINTS);
        let uses = uses_at(&a, &file, POINTS.find("x: i32,").unwrap()).unwrap();
        assert_eq!(
            places(POINTS, &uses.matching),
            ["x: i32, pub y: i32 }", "x, y: 1 } }", "x + p.y }"]
        );
        // The type: its definition, the return type, the literal and the annotation.
        let uses = uses_at(&a, &file, POINTS.find("Point {").unwrap() + 1).unwrap();
        assert_eq!(uses.matching.len(), 4);
    }

    #[test]
    fn renaming_one_side_of_a_shorthand_writes_both() {
        let (_dir, file, a) = analyzed(POINTS);
        let field = uses_at(&a, &file, POINTS.find("x: i32,").unwrap()).unwrap();
        let renamed = applied(POINTS, &rename_edit(&field, "z"));
        assert!(renamed.contains("struct Point { pub z: i32"), "{renamed}");
        assert!(renamed.contains("Point { z: x, y: 1 }"), "{renamed}");
        assert!(renamed.contains("p.z + p.y"), "{renamed}");

        let param = uses_at(&a, &file, POINTS.find("x: i32)").unwrap()).unwrap();
        let renamed = applied(POINTS, &rename_edit(&param, "n"));
        assert!(
            renamed.contains("fn make(n: i32) -> Point { Point { x: n, y: 1 } }"),
            "{renamed}"
        );
    }

    #[test]
    fn a_trait_method_is_renamed_with_its_implementations() {
        let source = "trait Shape { fn area(self: &Self) -> i32; }\n\
                      struct Square { side: i32 }\n\
                      impl Shape for Square { fn area(self: &Square) -> i32 { self.side } }\n\
                      fn main() -> i32 { let s = Square { side: 2 }; s.area() }\n";
        let (_dir, file, a) = analyzed(source);
        let call = uses_at(&a, &file, source.find("area()").unwrap()).unwrap();
        assert_eq!(call.matching.len(), 3);
        let renamed = applied(source, &rename_edit(&call, "size"));
        assert_eq!(renamed.matches("size").count(), 3, "{renamed}");
        assert!(renamable(&a, &call).is_ok());
    }

    #[test]
    fn what_isnt_yours_cant_be_renamed() {
        let source = "fn main() -> i32 { let x: i32 = 1; x }\n";
        let (_dir, file, a) = analyzed(source);
        let builtin = uses_at(&a, &file, source.find("i32 =").unwrap());
        assert!(builtin.is_none_or(|u| renamable(&a, &u).is_err()));
        assert!(valid_name("fn").is_err());
        assert!(valid_name("two words").is_err());
        assert!(valid_name("fine_name2").is_ok());
    }
}

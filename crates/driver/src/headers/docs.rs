use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// Comments for declarations, read from the text of the file each one is in (the
/// header, or one it includes) at the positions clang gives.
pub(super) struct Docs {
    /// Each declaration's file, by clang's node id.
    pub files: HashMap<String, String>,
    /// The text of those files, by name.
    texts: HashMap<String, String>,
}

impl Docs {
    pub fn of(json: &Value) -> Docs {
        let mut files = HashMap::new();
        files_of(json, &mut String::new(), &mut files);
        let texts = files
            .values()
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|file| Some((file.clone(), std::fs::read_to_string(file).ok()?)))
            .collect();
        Docs { files, texts }
    }

    /// The text of a declaration's file, and `start..end` of the declaration in it, if
    /// it's really there.
    fn range(&self, decl: &Value) -> Option<(&str, usize, usize)> {
        let text = self.texts.get(self.files.get(decl["id"].as_str()?)?)?;
        let (start, _) = location(&decl["range"]["begin"])?;
        let (end, len) = location(&decl["range"]["end"])?;
        let end = end + len;
        let name = decl["name"].as_str()?;
        text.get(start..end)?
            .contains(name)
            .then_some((text.as_str(), start, end))
    }

    /// The file and line a declaration's name is on.
    pub fn origin(&self, decl: &Value) -> Option<(String, usize)> {
        let file = self.files.get(decl["id"].as_str()?)?;
        let (offset, _) = location(&decl["loc"])?;
        let before = self.texts.get(file)?.get(..offset)?;
        Some((file.clone(), before.matches('\n').count() + 1))
    }

    /// A function's or field's comment: on its own line after it (`int x; // the x`),
    /// or else a doc comment (`///`, `/** */`) right above it.
    pub fn of_member(&self, decl: &Value) -> Vec<String> {
        let trailing = self.after(decl);
        if trailing.is_empty() {
            self.above(decl, false)
        } else {
            trailing
        }
    }

    fn after(&self, decl: &Value) -> Vec<String> {
        let Some((text, _, end)) = self.range(decl) else {
            return Vec::new();
        };
        let rest = &text[end..];
        let line = &rest[..rest.find('\n').unwrap_or(rest.len())];
        let line = line.trim_start_matches([';', ',', ' ', '\t']);
        comment_lines(line)
    }

    /// The comment directly above a declaration: a `/* */` block, or `//` or one-line
    /// `/* */` lines. Unless `any`, `//` lines only count as `///`: a plain `//` line
    /// above a function is usually a section heading.
    pub fn above(&self, decl: &Value, any: bool) -> Vec<String> {
        let Some((text, start, _)) = self.range(decl) else {
            return Vec::new();
        };
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let before = text[..line_start].trim_end_matches(['\n', '\r']);
        let last = before.rsplit('\n').next().unwrap_or_default().trim();

        if last.ends_with("*/") && !is_one_line_block(last) {
            let Some(open) = before.rfind("/*") else {
                return Vec::new();
            };
            // Only a comment on lines of its own: not the end of `int x; /* … */`.
            let line = before[..open].rsplit('\n').next().unwrap_or_default();
            if !line.trim().is_empty() {
                return Vec::new();
            }
            return block_comment(&before[open..]);
        }

        let mut lines: Vec<&str> = Vec::new();
        for line in before.lines().rev() {
            let trimmed = line.trim();
            if !(trimmed.starts_with("//") || is_one_line_block(trimmed)) {
                break;
            }
            lines.push(trimmed);
        }
        lines.reverse();
        let heading = |l: &str| l.starts_with("//") && !l.starts_with("///");
        if lines.is_empty() || (!any && heading(lines[0])) {
            return Vec::new();
        }
        lines.iter().flat_map(|line| comment_lines(line)).collect()
    }

    /// Doc comment lines, indented.
    pub fn write(&self, lines: &[String], indent: &str) -> String {
        lines
            .iter()
            .map(|line| format!("{indent}/// {line}").trim_end().to_string() + "\n")
            .collect()
    }
}

fn is_one_line_block(line: &str) -> bool {
    line.starts_with("/*") && line.ends_with("*/")
}

/// The file each node in clang's JSON is in, by node id. clang only names a location's
/// file when it differs from the location written before it, so this follows them in the
/// order clang writes them: `loc`, then `range`, then the nodes inside.
fn files_of(node: &Value, current: &mut String, files: &mut HashMap<String, String>) {
    let note = |at: &Value, current: &mut String| {
        for at in [&at["spellingLoc"], &at["expansionLoc"], at] {
            if let Some(file) = at["file"].as_str() {
                *current = file.to_string();
            }
        }
    };
    note(&node["loc"], current);
    note(&node["range"]["begin"], current);
    // Where the declaration starts: with a macro, where the macro is used.
    if let Some(id) = node["id"].as_str() {
        files.insert(id.to_string(), current.clone());
    }
    note(&node["range"]["end"], current);
    for inner in node["inner"].as_array().into_iter().flatten() {
        files_of(inner, current, files);
    }
}

/// A source location's offset and token length. One inside a macro (`MA_API int f();`)
/// is where the macro is used: its `expansionLoc`.
fn location(at: &Value) -> Option<(usize, usize)> {
    let at = if at["expansionLoc"].is_object() {
        &at["expansionLoc"]
    } else {
        at
    };
    Some((
        at["offset"].as_u64()? as usize,
        at["tokLen"].as_u64().unwrap_or(0) as usize,
    ))
}

/// The lines of a `/* */` comment, keeping blank lines and indentation (the docs can be
/// Markdown, with code blocks), without the `/*`, `*/` and a ` * ` margin.
fn block_comment(comment: &str) -> Vec<String> {
    let inner = comment
        .trim_start_matches('/')
        .trim_start_matches(['*', '!', '<'])
        .trim_end_matches("*/")
        .trim_end_matches('*');
    let mut lines: Vec<&str> = inner.lines().map(str::trim_end).collect();
    let has_margin =
        (lines.iter().skip(1)).all(|l| l.trim().is_empty() || l.trim_start().starts_with('*'));
    if has_margin {
        lines = (lines.iter().enumerate())
            .map(|(i, l)| {
                let t = l.trim_start();
                if i == 0 {
                    t
                } else {
                    let t = t.strip_prefix('*').unwrap_or(t);
                    t.strip_prefix(' ').unwrap_or(t)
                }
            })
            .collect();
    }
    let indent = (lines.iter().skip(1))
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = (lines.iter().enumerate())
        .map(|(i, l)| {
            if i == 0 {
                l.trim().to_string()
            } else {
                l.get(indent..).unwrap_or(l.trim_start()).to_string()
            }
        })
        .collect();
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

/// The text of a comment line: `// text`, `/// text`, `//< text`, `/* text */`, `* text`.
pub(super) fn comment_lines(line: &str) -> Vec<String> {
    let line = line.trim();
    let text = if let Some(rest) = line.strip_prefix("//") {
        rest.trim_start_matches(['/', '!', '<'])
    } else if line.starts_with("/*") || line.starts_with('*') || line.ends_with("*/") {
        line.trim_start_matches('/')
            .trim_start_matches('*')
            .trim_start_matches('!')
            .trim_start_matches('<')
            .trim_end_matches("*/")
            .trim_end_matches('*')
    } else {
        return Vec::new();
    };
    let text = text.trim();
    if text.is_empty() {
        Vec::new()
    } else {
        vec![text.to_string()]
    }
}

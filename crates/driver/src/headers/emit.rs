//! A header's declarations, written out as som source.
//!
//! Only functions the library actually exports are written, which leaves out macros,
//! `static inline` helpers, and whatever the header pulls in from other headers. A
//! function using something som can't express yet (arrays, unions, `long double`) is
//! skipped, with a comment in the generated source saying why.

use serde_json::Value;
use som_base::diagnostics::{pascal_case, snake_case};
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use super::ctype::Converter;
use super::docs::Docs;
use super::{LIBC, identifier, macros, top_level};

/// The header's declarations as som source, and where each is declared: lines of
/// `name\tfile\tline`, for [`super::origins`].
pub(super) fn generate(
    json: &Value,
    library: Option<&str>,
    header: &str,
    exported: Option<&HashSet<String>>,
    preprocessed: &str,
) -> (String, String) {
    let decls = top_level(json);
    let mut emitter = Emitter {
        decls,
        docs: Docs::of(json),
        library,
        header,
        origins: String::new(),
    };
    let mut c = Converter::of(decls);
    let functions = emitter.functions(&mut c, exported);

    let mut out = format!("// Imported from {header} by the som compiler.\n");
    out.push_str(&functions.skipped);
    out.push('\n');
    out.push_str(&emitter.structs(&c));
    out.push_str(&emitter.constants(&c, &functions.names, preprocessed));
    // The C library's enums are left out, as they're mostly the system's.
    if library != Some(LIBC) {
        out.push_str(&emitter.enums(&c));
    }
    out.push_str(&emitter.handles(&c));
    out.push_str(&emitter.extern_block(&functions.text));
    (out, emitter.origins)
}

struct Emitter<'a> {
    decls: &'a [Value],
    docs: Docs,
    library: Option<&'a str>,
    header: &'a str,
    /// Lines of `name\tfile\tline`.
    origins: String,
}

struct Functions {
    /// Each declaration, indented for the `extern` block.
    text: String,
    /// `// skipped …` lines, saying why.
    skipped: String,
    /// som name → C name, of every function written or skipped for its name.
    names: HashMap<String, String>,
}

impl Emitter<'_> {
    fn note_origin(&mut self, name: &str, decl: &Value) {
        if let Some((file, line)) = self.docs.origin(decl) {
            let _ = writeln!(self.origins, "{name}\t{file}\t{line}");
        }
    }

    /// A top-level declaration's doc comment, noting where it's declared.
    fn doc_above(&mut self, name: &str, decl: &Value) -> String {
        let doc = self.docs.write(&self.docs.above(decl, true), "");
        self.note_origin(name, decl);
        doc
    }

    fn functions(&mut self, c: &mut Converter, exported: Option<&HashSet<String>>) -> Functions {
        let mut functions = Functions {
            text: String::new(),
            skipped: String::new(),
            names: HashMap::new(),
        };
        for decl in self.decls {
            if decl["kind"] != "FunctionDecl" || decl["isImplicit"] == true {
                continue;
            }
            let Some(name) = decl["name"].as_str() else {
                continue;
            };
            if exported.is_some_and(|names| !names.contains(name)) {
                continue;
            }
            // The C library's own internals: `__error`, `_exit`'s relatives.
            if self.library == Some(LIBC) && name.starts_with('_') {
                continue;
            }
            // `InitWindow` is `init_window`, and links back to it.
            let som_name = identifier(&snake_case(name));
            if let Some(other) = functions.names.get(&som_name) {
                if other != name {
                    let _ = writeln!(
                        functions.skipped,
                        "// skipped {name}: in som it would be called {som_name}, like {other}"
                    );
                }
                continue;
            }
            functions.names.insert(som_name.clone(), name.to_string());
            match c.function(decl, &som_name) {
                Ok(text) => {
                    let doc = self.docs.write(&self.docs.of_member(decl), "    ");
                    functions.text.push_str(&doc);
                    self.note_origin(&som_name, decl);
                    let _ = writeln!(functions.text, "    {text}");
                }
                Err(reason) => {
                    let _ = writeln!(functions.skipped, "// skipped {name}: {reason}");
                }
            }
        }
        functions
    }

    fn structs(&mut self, c: &Converter) -> String {
        let mut out = String::new();
        for (name, fields) in &c.structs {
            let tag = c.tags[name].as_str();
            let decl = (self.decls.iter())
                .find(|d| d["kind"] == "RecordDecl" && d["name"] == tag && d["inner"].is_array());
            let field_decls: Vec<&Value> = decl
                .and_then(|d| d["inner"].as_array())
                .map(|inner| inner.iter().filter(|f| f["kind"] == "FieldDecl").collect())
                .unwrap_or_default();
            if let Some(decl) = decl {
                out.push_str(&self.doc_above(name, decl));
            }
            let _ = writeln!(out, "pub struct {name} {{");
            for (i, (field, (_, ty))) in field_names(fields).iter().zip(fields).enumerate() {
                if let Some(field_decl) = field_decls.get(i) {
                    out.push_str(&self.docs.write(&self.docs.of_member(field_decl), "    "));
                }
                let _ = writeln!(out, "    pub {field}: {ty},");
            }
            out.push_str("}\n\n");
        }
        out
    }

    /// `#define`s that stand for values: the library's own, or for the C library, any not
    /// its internals'.
    fn constants(
        &mut self,
        c: &Converter,
        functions: &HashMap<String, String>,
        preprocessed: &str,
    ) -> String {
        let taken: HashSet<String> = (functions.keys())
            .chain(c.structs.keys())
            .chain(c.handles.keys())
            .cloned()
            .collect();
        let (library, header) = (self.library, self.header);
        let wanted = |file: &str, name: &str| match library {
            Some(LIBC) => !file.starts_with('<') && !name.starts_with('_'),
            _ => macros::same_file(file, header),
        };
        let record = |ty: &str| {
            let (name, fields) = (c.structs.iter()).find(|(name, _)| {
                name.as_str() == ty || c.tags.get(*name).is_some_and(|tag| tag == ty)
            })?;
            Some(macros::Record {
                name: name.clone(),
                fields: field_names(fields)
                    .into_iter()
                    .zip(fields.iter().map(|(_, ty)| ty.clone()))
                    .collect(),
            })
        };
        let (constants, origins) = macros::constants(preprocessed, &wanted, &record, &taken);
        self.origins.push_str(&origins);
        constants
    }

    /// C's enums as som enums whose variants convert to C's numbers with `as`:
    /// `typedef enum { KEY_NULL = 0, KEY_W = 87 } KeyboardKey;` is
    /// `pub enum KeyboardKey { Null = 0, W = 87 }`, and `KeyboardKey::W as i32` is 87.
    /// Functions still take C's enums as `i32`, as C does. Only the header's own enums
    /// are written, not ones it pulls in from the system's.
    fn enums(&mut self, c: &Converter) -> String {
        let taken: HashSet<&str> = (c.structs.keys().chain(c.handles.keys()))
            .map(String::as_str)
            .collect();
        let header_file = Path::new(self.header).file_name();
        let typedefs = enum_typedefs(self.decls);
        let mut out = String::new();
        let mut written: HashSet<String> = HashSet::new();
        for decl in self.decls.iter().filter(|d| d["kind"] == "EnumDecl") {
            let Some(constants) = decl["inner"].as_array() else {
                continue;
            };
            let Some(id) = decl["id"].as_str() else {
                continue;
            };
            let in_header = (self.docs.files.get(id))
                .is_some_and(|file| Path::new(file).file_name() == header_file);
            let Some(name) = typedefs.get(id).copied().or(decl["name"].as_str()) else {
                continue;
            };
            let name = identifier(&pascal_case(name));
            if !in_header || taken.contains(name.as_str()) || !written.insert(name.clone()) {
                continue;
            }
            let constants: Vec<&Value> = (constants.iter())
                .filter(|c| c["kind"] == "EnumConstantDecl")
                .collect();
            let c_names: Vec<&str> = constants
                .iter()
                .filter_map(|c| c["name"].as_str())
                .collect();
            if c_names.len() != constants.len() || c_names.is_empty() {
                continue;
            }
            let Some(variants) = variant_names(&c_names) else {
                continue;
            };

            out.push_str(&self.doc_above(&name, decl));
            let _ = writeln!(out, "pub enum {name} {{");
            let mut value: i128 = -1;
            for (constant, variant) in constants.iter().zip(&variants) {
                value = constant_value(constant).unwrap_or(value + 1);
                out.push_str(&self.docs.write(&self.docs.of_member(constant), "    "));
                let _ = writeln!(out, "    {} = {value},", identifier(variant));
            }
            out.push_str("}\n\n");
        }
        out
    }

    fn handles(&mut self, c: &Converter) -> String {
        let mut out = String::new();
        for (name, tag) in &c.handles {
            let decl = (self.decls.iter())
                .find(|d| d["kind"] == "RecordDecl" && d["name"] == tag.as_str());
            if let Some(decl) = decl {
                out.push_str(&self.doc_above(name, decl));
            }
            let _ = writeln!(out, "pub extern struct {name};\n");
        }
        out
    }

    fn extern_block(&self, functions: &str) -> String {
        let header = self.header;
        // Naming the header marks these as coming from it, so they aren't checked against it again.
        let mut out = match self.library {
            Some(library) => format!("extern {library:?} from {header:?} {{\n"),
            None => format!("extern from {header:?} {{\n"),
        };
        out.push_str(functions);
        out.push_str("}\n");
        out
    }
}

/// `typedef enum { … } Name;`: the name each enum is known by, by the enum's id.
fn enum_typedefs(decls: &[Value]) -> HashMap<&str, &str> {
    let mut typedefs = HashMap::new();
    for decl in decls.iter().filter(|d| d["kind"] == "TypedefDecl") {
        let owned = &decl["inner"][0]["ownedTagDecl"];
        if owned["kind"] == "EnumDecl"
            && let (Some(id), Some(name)) = (owned["id"].as_str(), decl["name"].as_str())
        {
            typedefs.entry(id).or_insert(name);
        }
    }
    typedefs
}

/// The som variants for C's enum constants, without what their names share: `KEY_NULL`
/// and `KEY_W` are `Null` and `W`. `None` if they can't all be told apart.
fn variant_names(c_names: &[&str]) -> Option<Vec<String>> {
    let shared = shared_prefix(c_names);
    let variant = |c_name: &str| {
        let rest = c_name.strip_prefix(shared).unwrap_or(c_name);
        // `KEY_LEFT_BRACKET`'s words, whatever their case: `LeftBracket`.
        let words = match rest.chars().any(|c| c.is_ascii_lowercase()) {
            true => rest.to_string(),
            false => rest.to_ascii_lowercase(),
        };
        pascal_case(&words)
    };
    let mut variants: Vec<String> = c_names.iter().map(|n| variant(n)).collect();
    // A number first, `KEY_0`, can't start a name: keep C's names whole.
    let unusable = |v: &String| v.is_empty() || v.starts_with(|c: char| c.is_ascii_digit());
    if variants.iter().any(unusable) {
        variants = c_names.iter().map(|n| pascal_case(n)).collect();
    }
    let distinct: HashSet<&String> = variants.iter().collect();
    (distinct.len() == variants.len() && !variants.iter().any(unusable)).then_some(variants)
}

/// What every name starts with, up to an `_`: `KEY_` of `KEY_NULL` and `KEY_W`.
fn shared_prefix<'a>(names: &[&'a str]) -> &'a str {
    let mut shared = names[0];
    for other in &names[1..] {
        let common = shared
            .char_indices()
            .zip(other.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8());
        shared = &shared[..common];
    }
    shared.rfind('_').map_or("", |i| &shared[..=i])
}

/// The number written for an enum constant, `= 87`, as clang worked it out.
fn constant_value(constant: &Value) -> Option<i128> {
    fn find(node: &Value) -> Option<i128> {
        if node["kind"] == "ConstantExpr"
            && let Some(value) = node["value"].as_str()
        {
            return value.parse().ok();
        }
        node["inner"].as_array()?.iter().find_map(find)
    }
    find(constant)
}

/// A struct's field names in som: snake_case too, `sampleRate` is `sample_rate`. Only the
/// layout has to match C's, so the names are free; one that would clash keeps its C name.
fn field_names(fields: &[(String, String)]) -> Vec<String> {
    let snake: Vec<String> = fields.iter().map(|(f, _)| snake_case(f)).collect();
    (fields.iter().enumerate())
        .map(|(i, (field, _))| {
            let clashes = snake.iter().filter(|s| **s == snake[i]).count() > 1
                || fields.iter().any(|(f, _)| *f == snake[i] && f != field);
            identifier(if clashes { field } else { &snake[i] })
        })
        .collect()
}

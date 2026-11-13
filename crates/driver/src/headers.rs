//! `extern "raylib" from "raylib.h";`: declarations imported from a C header.
//! `extern "raylib";` is short for exactly that.
//!
//! The header is read by clang (`-Xclang -ast-dump=json`), and its functions and
//! structs are written out as som source, which is parsed into a module named
//! after the library: `raylib::InitWindow`, `raylib::Color`. Everything after
//! loading treats them as if they'd been written by hand.
//!
//! Only functions the library actually exports are imported, which leaves out
//! macros, `static inline` helpers, and whatever the header pulls in from other
//! headers. C types map onto som's:
//!
//! | C                                   | som            |
//! |-------------------------------------|----------------|
//! | `char`, `short`, `int`, `long`, …   | `i8` … `u64`   |
//! | `float`, `double`, `bool`           | `f32`, `f64`, `bool` |
//! | `const char *`                      | `&str`         |
//! | parameter `const T *`, `T *`        | `&T`, `&mut T` |
//! | any other pointer                   | `*`            |
//! | `struct Color` (by value)           | `Color`, with the same fields |
//! | `enum …`                            | `i32`          |
//!
//! A function using something som can't express yet (arrays, unions, `long
//! double`) is skipped, with a comment in the generated source saying why.
//!
//! The generated source is kept in `.som/<library>.som` next to the program. It's
//! reused as long as the header, the library and the compiler are unchanged, and
//! it's a normal file to read: errors and go-to-definition point into it.

use serde_json::Value;
use som_ast::{Ast, Func, Ident, Item, Path as SomPath, Span, Use};
use som_base::diagnostics::{pascal_case, snake_case};
use som_base::message::{self as m, Message};
use som_base::{Source, msg};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use crate::LoadError;
use crate::libraries::{self, LibraryError};

/// Sets up the library modules for `module`'s `extern` blocks. A library's functions
/// go in a module named after it (`raylib::InitWindow`), whether they're written by
/// hand or imported from its header; `use extern` also `use`s that module. Headers are
/// imported once each, tracked in `imported`. Returns the names of the modules created.
pub fn import(
    ast: &mut Ast,
    module: usize,
    library_dirs: &[PathBuf],
    imported: &mut HashSet<String>,
    errors: &mut Vec<LoadError>,
) -> Vec<String> {
    let root = ast.modules[0].path.clone();
    let blocks: Vec<usize> = ast.modules[module]
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, Item::Extern(b) if b.library.is_some() || b.header.is_some()))
        .map(|(i, _)| i)
        .collect();

    let mut created = Vec::new();
    for index in blocks {
        let Item::Extern(block) = &ast.modules[module].items[index] else {
            unreachable!()
        };
        let (library, header, use_module) = (
            block.library.clone(),
            block.header.clone(),
            block.use_module,
        );
        let span = block
            .library_span
            .clone()
            .or_else(|| header.as_ref().map(|(_, s)| s.clone()));
        let name = module_name(library.as_deref(), header.as_ref().map_or("", |(h, _)| h));

        let target = match ast.modules.iter().position(|m| *m.name == name) {
            Some(existing) => existing,
            None => {
                created.push(name.clone());
                ast.add_module(name.as_str(), PathBuf::from(format!("<{name}>")))
            }
        };
        if let Item::Extern(block) = &mut ast.modules[module].items[index] {
            block.module = Some(target);
        }
        // `use extern "raylib" …` is the block plus `use raylib;`.
        if use_module && let Some(span) = span {
            let path = SomPath {
                segments: vec![Ident {
                    name: name.as_str().into(),
                    span,
                }],
            };
            ast.modules[module].items.push(Item::Use(Use { path }));
        }

        if let Some((header, span)) = header
            && imported.insert(name.clone())
        {
            import_header(
                ast,
                target,
                library.as_deref(),
                &header,
                span,
                &root,
                library_dirs,
                errors,
            );
        }
    }
    created
}

/// Parses a header's declarations, as som, into `module`.
#[allow(clippy::too_many_arguments)]
fn import_header(
    ast: &mut Ast,
    module: usize,
    library: Option<&str>,
    header: &str,
    span: Span,
    root: &Path,
    library_dirs: &[PathBuf],
    errors: &mut Vec<LoadError>,
) {
    let path = match find_header(header, root, library_dirs) {
        Ok(path) => path,
        Err(searched) => {
            errors.push(LoadError::HeaderNotFound {
                header: header.to_string(),
                span,
                searched,
            });
            return;
        }
    };
    let library_file =
        library.and_then(|library| libraries::find(library, root, library_dirs).ok());

    // The generated source is kept in `.som/<module>.som` next to the program, for reuse
    // and for reading. Its first line says what it was made from; if that still matches,
    // the header doesn't need to be read again.
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let module_name = ast.modules[module].name.to_string();
    let cache = root.join(".som").join(format!("{module_name}.som"));
    let stamp = cache_stamp(&path, library_file.as_deref());
    let cached = std::fs::read_to_string(&cache)
        .ok()
        .filter(|text| text.lines().next() == Some(stamp.as_str()));

    let text = match cached {
        Some(text) => text,
        None => {
            // Only what the library exports; if it can't be found, the library check reports that.
            let exported: Option<HashSet<String>> = library_file
                .as_deref()
                .and_then(|file| libraries::read_symbols(file).ok())
                .map(|names| names.into_iter().collect());
            let (generated, origins) = match clang_ast(&path, root, library_dirs) {
                Ok(json) => generate(&json, library, header, exported.as_ref()),
                Err(message) => {
                    errors.push(LoadError::HeaderFailed {
                        header: header.to_string(),
                        span,
                        message,
                    });
                    return;
                }
            };
            let text = format!("{stamp}\n{generated}");
            // A cache that can't be written only costs time next build.
            let _ = std::fs::create_dir_all(cache.parent().expect("in .som"))
                .and_then(|()| std::fs::write(&cache, &text))
                .and_then(|()| std::fs::write(cache.with_extension("origins"), origins));
            text
        }
    };
    ast.modules[module].path = cache.parent().expect("in .som").to_path_buf();
    let source = Source::from_raw_at(text, cache);
    let parse_errors = som_parser::parse_file(Arc::new(source), ast, module);
    errors.extend(parse_errors.into_iter().map(LoadError::Parse));
}

/// Where each function in a generated file (`.som/raylib.som`) is declared in C: its
/// som name → the header and line, from the `.origins` file written next to it.
pub fn origins(generated: &Path) -> HashMap<String, (PathBuf, usize)> {
    let text = std::fs::read_to_string(generated.with_extension("origins")).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let (name, file, line) = (parts.next()?, parts.next()?, parts.next()?);
            Some((name.to_string(), (PathBuf::from(file), line.parse().ok()?)))
        })
        .collect()
}

/// The module a header's declarations go in: the library's name, or the header's.
fn module_name(library: Option<&str>, header: &str) -> String {
    let name = match library {
        Some(library) => Path::new(library)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(library),
        None => Path::new(header)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(header),
    };
    let name = name
        .strip_prefix("lib")
        .filter(|_| library.is_some_and(|l| l.contains('/')))
        .unwrap_or(name);
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Compares hand-written `extern "library" { … }` declarations with the library's own
/// header, `library.h`, if it can be found. Blocks that import a header are skipped:
/// their declarations come from it.
pub fn check_declarations(ast: &Ast, library_dirs: &[PathBuf]) -> Vec<LibraryError> {
    let root = ast.modules[0].path.clone();
    let mut headers: HashMap<String, Option<Declarations>> = HashMap::new();
    let mut errors = Vec::new();

    for item in ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let (Some(library), None) = (&block.library, &block.header) else {
            continue;
        };
        let header = format!("{library}.h");
        let declared = headers.entry(library.clone()).or_insert_with(|| {
            let path = find_header(&header, &root, library_dirs).ok()?;
            let json = clang_ast(&path, &root, library_dirs).ok()?;
            Some(declarations(&json))
        });
        let Some(declared) = declared else { continue };

        for &id in &block.funcs {
            let func = &ast.funcs[id];
            // Compared by the C symbol it links to: `init_window` is `InitWindow` there.
            let symbol = func.link_name.as_deref().unwrap_or(&func.name.name);
            let Some(expected) = declared.functions.get(symbol) else {
                continue;
            };
            if let Some(problem) = mismatch(ast, func, expected, &declared.structs) {
                errors.push(LibraryError::HeaderMismatch {
                    function: func.name.name.to_string(),
                    header: header.clone(),
                    declared_there: expected.text(&func.name.name),
                    problem,
                    span: func.name.span.clone(),
                });
            }
        }
    }
    errors
}

/// What a header declares, in som terms.
struct Declarations {
    /// Every function som can express, by name.
    functions: HashMap<String, Signature>,
    /// The structs those functions use: name → field types.
    structs: BTreeMap<String, Vec<(String, String)>>,
}

fn declarations(json: &Value) -> Declarations {
    let decls = json["inner"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut c = Converter::of(decls);
    let functions = decls
        .iter()
        .filter(|d| d["kind"] == "FunctionDecl")
        .filter_map(|d| Some((d["name"].as_str()?.to_string(), c.signature(d).ok()?)))
        .collect();
    Declarations {
        functions,
        structs: c.structs,
    }
}

/// How a declaration differs from the header's, if it does.
fn mismatch(
    ast: &Ast,
    func: &Func,
    expected: &Signature,
    structs: &BTreeMap<String, Vec<(String, String)>>,
) -> Option<Message> {
    let compatible = |written: &str, header: &str| compatible(ast, structs, written, header);
    let types: Vec<String> = func
        .params
        .iter()
        .map(|p| p.ty.map_or("_".into(), |t| ast.type_text(t)))
        .collect();
    if types.len() != expected.params.len() {
        let n = expected.params.len();
        return Some(msg![format!(
            "takes {n} parameter{} there, not {}",
            if n == 1 { "" } else { "s" },
            types.len()
        )]);
    }
    for (written, (name, ty)) in types.iter().zip(&expected.params) {
        if !compatible(written, ty) {
            return Some(msg![
                "parameter ",
                m::var(name),
                " is ",
                m::ty(ty),
                " there, not ",
                m::ty(written)
            ]);
        }
    }
    match (expected.variadic, func.variadic) {
        (true, false) => return Some(msg!["takes extra arguments (", m::code("..."), ") there"]),
        (false, true) => return Some(msg!["doesn't take extra arguments there"]),
        _ => {}
    }
    let returns = func.returns.map_or("()".into(), |t| ast.type_text(t));
    if !compatible(&returns, &expected.returns) {
        return Some(if expected.returns == "()" {
            msg!["returns nothing there"]
        } else {
            msg![
                "returns ",
                m::ty(&expected.returns),
                " there, not ",
                m::ty(&returns)
            ]
        });
    }
    None
}

/// Whether a type written in som agrees with the header's. All pointers are alike to C,
/// so `*` and `&str` are interchangeable, and a struct agrees with another struct whose
/// fields have the same types in the same order, whatever either is called.
fn compatible(
    ast: &Ast,
    structs: &BTreeMap<String, Vec<(String, String)>>,
    written: &str,
    header: &str,
) -> bool {
    let pointer = |t: &str| t == "*" || t.starts_with('&');
    if written == header || (pointer(written) && pointer(header)) {
        return true;
    }
    let Some(theirs) = structs.get(header) else {
        return false;
    };
    // The written name's last segment: `geo::Shade` is the struct `Shade`.
    let name = written.rsplit("::").next().unwrap_or(written);
    ast.structs.iter().any(|(_, ours)| {
        *ours.name.name == *name
            && ours.fields.len() == theirs.len()
            && ours
                .fields
                .iter()
                .zip(theirs)
                .all(|(field, (_, ty))| compatible(ast, structs, &ast.type_text(field.ty), ty))
    })
}

/// Bumped whenever the generated source changes, so old caches are regenerated.
const GENERATOR_VERSION: u32 = 12;

/// The first line of a cached import: everything it depends on. Any change to the header,
/// the library or the generator makes it stale.
fn cache_stamp(header: &Path, library: Option<&Path>) -> String {
    let modified = |path: &Path| {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };
    let mut stamp = format!(
        "// som import v{GENERATOR_VERSION}.{}: {} @{}",
        env!("CARGO_PKG_VERSION"),
        header.display(),
        modified(header)
    );
    if let Some(library) = library {
        let _ = write!(stamp, ", {} @{}", library.display(), modified(library));
    }
    stamp
}

/// The header file, or the directories that were searched for it: the program's
/// directory, then `CPATH` and `C_INCLUDE_PATH`, then the `include` directory next to
/// each library directory. A bare
/// `name.h` is also looked for as `name/name.h`, where packages like Homebrew's
/// `miniaudio` put it.
pub(crate) fn find_header(
    header: &str,
    root: &Path,
    library_dirs: &[PathBuf],
) -> Result<PathBuf, Vec<PathBuf>> {
    let dirs = include_dirs(root, library_dirs);
    let nested = Path::new(header)
        .file_stem()
        .filter(|_| !header.contains('/'))
        .map(|stem| Path::new(stem).join(header));
    dirs.iter()
        .flat_map(|dir| [Some(dir.join(header)), nested.as_ref().map(|n| dir.join(n))])
        .flatten()
        .find(|path| path.is_file())
        .ok_or(dirs)
}

/// The libraries `extern "name";` could import for a program in `root`: those with
/// both a library file (`libraylib.dylib`) and a header (`raylib.h`) where som looks,
/// each with the file it would link.
pub fn importable_libraries(root: &Path, library_dirs: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut names: Vec<String> = libraries::search_dirs(root, library_dirs)
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| {
            let file = entry.ok()?.file_name().into_string().ok()?;
            let name = file.strip_prefix("lib")?;
            let name = [".dylib", ".so", ".a"]
                .iter()
                .find_map(|ext| name.strip_suffix(ext))?;
            // `libgc.1.dylib` is a version of `libgc.dylib`.
            (!name.contains('.')).then(|| name.to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names.retain(|name| find_header(&format!("{name}.h"), root, library_dirs).is_ok());
    names
        .into_iter()
        .filter_map(|name| {
            let path = libraries::find(&name, root, library_dirs).ok()?;
            Some((name, path))
        })
        .collect()
}

/// Where headers are looked for: the program's directory, then `CPATH` and
/// `C_INCLUDE_PATH`, then the `-isystem` and `-I` directories in Nix's
/// `NIX_CFLAGS_COMPILE` (packages like sqlite keep their headers apart from their
/// libraries), then the `include` directory next to each library directory.
fn include_dirs(root: &Path, library_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let from_env = ["CPATH", "C_INCLUDE_PATH"]
        .into_iter()
        .filter_map(std::env::var_os)
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        // To clang an empty entry is the current directory; the program's comes first anyway.
        .filter(|dir| !dir.as_os_str().is_empty());
    let from_nix = std::env::var("NIX_CFLAGS_COMPILE")
        .map(|flags| libraries::flag_dirs(&flags, &["-isystem", "-I"]))
        .unwrap_or_default();
    let next_to_libraries = libraries::search_dirs(root, library_dirs)
        .into_iter()
        .filter_map(|dir| Some(dir.parent()?.join("include")));
    let mut dirs = vec![root.to_path_buf()];
    for include in from_env.chain(from_nix).chain(next_to_libraries) {
        if !dirs.contains(&include) {
            dirs.push(include);
        }
    }
    dirs
}

/// The header's syntax tree, as clang's JSON. The header's own `#include <…>`s are
/// looked for where the header was: `gc.h` includes `<gc/gc.h>`.
fn clang_ast(header: &Path, root: &Path, library_dirs: &[PathBuf]) -> Result<Value, String> {
    let compiler = std::env::var("SOM_CC").unwrap_or_else(|_| "cc".into());
    let output = Command::new(&compiler)
        .args(["-Xclang", "-ast-dump=json", "-fsyntax-only", "-x", "c"])
        .args(
            include_dirs(root, library_dirs)
                .iter()
                .filter(|dir| dir.is_dir())
                .map(|dir| format!("-I{}", dir.display())),
        )
        .arg(header)
        .output()
        .map_err(|e| format!("could not run `{compiler}`: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first = stderr
            .lines()
            .find(|l| l.contains("error"))
            .unwrap_or("it reported errors");
        return Err(format!("clang could not read it: {}", first.trim()));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("could not read clang's output: {e}"))
}

/// som can't name these, so imported names that use them get a `_` after.
const SOM_KEYWORDS: &[&str] = &[
    "fn", "extern", "struct", "impl", "let", "if", "else", "mut", "use", "loop", "while", "for",
    "true", "false", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128",
    "usize", "f32", "f64", "bool", "char", "str",
];

fn identifier(name: &str) -> String {
    if SOM_KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// The header's declarations as som source, and where each function is declared: lines
/// of `name\tfile\tline`, for [`origins`].
fn generate(
    json: &Value,
    library: Option<&str>,
    header: &str,
    exported: Option<&HashSet<String>>,
) -> (String, String) {
    let docs = Docs::of(json);
    let mut origins = String::new();
    let decls = json["inner"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut c = Converter::of(decls);

    let mut functions = String::new();
    let mut skipped = String::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    for decl in decls {
        if decl["kind"] != "FunctionDecl" || decl["isImplicit"] == true {
            continue;
        }
        let Some(name) = decl["name"].as_str() else {
            continue;
        };
        if exported.is_some_and(|names| !names.contains(name)) {
            continue;
        }
        // som names are snake_case: `InitWindow` is `init_window`, and links back to it.
        let som_name = identifier(&snake_case(name));
        if let Some(other) = seen.get(&som_name) {
            if other != name {
                let _ = writeln!(
                    skipped,
                    "// skipped {name}: in som it would be called {som_name}, like {other}"
                );
            }
            continue;
        }
        seen.insert(som_name.clone(), name.to_string());
        match c.function(decl, &som_name) {
            Ok(text) => {
                functions.push_str(&docs.write(&docs.of_function(decl), "    "));
                // Where it's declared, for editors to go to.
                if let Some((file, line)) = docs.origin(decl) {
                    let _ = writeln!(origins, "{som_name}\t{file}\t{line}");
                }
                let _ = writeln!(functions, "    {text}");
            }
            Err(reason) => {
                let _ = writeln!(skipped, "// skipped {name}: {reason}");
            }
        }
    }

    let mut out = format!("// Imported from {header} by the som compiler.\n");
    if !skipped.is_empty() {
        out.push_str(&skipped);
    }
    out.push('\n');
    for (name, fields) in &c.structs {
        let tag = c.tags[name].as_str();
        let decl = decls
            .iter()
            .find(|d| d["kind"] == "RecordDecl" && d["name"] == tag && d["inner"].is_array());
        let field_decls: Vec<&Value> = decl
            .and_then(|d| d["inner"].as_array())
            .map(|inner| inner.iter().filter(|f| f["kind"] == "FieldDecl").collect())
            .unwrap_or_default();
        if let Some(decl) = decl {
            out.push_str(&docs.write(&docs.above(decl, true), ""));
            if let Some((file, line)) = docs.origin(decl) {
                let _ = writeln!(origins, "{name}\t{file}\t{line}");
            }
        }
        let _ = writeln!(out, "struct {name} {{");
        // Fields are snake_case too: `sampleRate` is `sample_rate`. Only the layout has to
        // match C's, so the names are free; one that would clash keeps its C name.
        let snake: Vec<String> = fields.iter().map(|(f, _)| snake_case(f)).collect();
        for (i, (field, ty)) in fields.iter().enumerate() {
            if let Some(field_decl) = field_decls.get(i) {
                out.push_str(&docs.write(&docs.of_function(field_decl), "    "));
            }
            let clashes = snake.iter().filter(|s| **s == snake[i]).count() > 1
                || fields.iter().any(|(f, _)| *f == snake[i] && f != field);
            let name = if clashes { field } else { &snake[i] };
            let _ = writeln!(out, "    {}: {ty},", identifier(name));
        }
        out.push_str("}\n\n");
    }
    // Naming the header marks these as coming from it, so they aren't checked against it again.
    match library {
        Some(library) => {
            let _ = writeln!(out, "extern {library:?} from {header:?} {{");
        }
        None => {
            let _ = writeln!(out, "extern from {header:?} {{");
        }
    }
    out.push_str(&functions);
    out.push_str("}\n");
    (out, origins)
}

/// A C function's signature in som types.
struct Signature {
    params: Vec<(String, String)>,
    variadic: bool,
    /// `()` for `void`.
    returns: String,
}

impl Signature {
    /// `fn name(a: i32, b: &str) -> f32`
    fn text(&self, name: &str) -> String {
        let mut params: Vec<String> = self
            .params
            .iter()
            .map(|(n, ty)| format!("{n}: {ty}"))
            .collect();
        if self.variadic {
            params.push("...".into());
        }
        let returns = if self.returns == "()" {
            String::new()
        } else {
            format!(" -> {}", self.returns)
        };
        format!("fn {name}({}){returns}", params.join(", "))
    }
}

/// Comments for declarations, read from the text of the file each one is in (the
/// header, or one it includes) at the positions clang gives.
struct Docs {
    /// Each declaration's file, by clang's node id.
    files: HashMap<String, String>,
    /// The text of those files, by name.
    texts: HashMap<String, String>,
}

impl Docs {
    fn of(json: &Value) -> Docs {
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
    fn origin(&self, decl: &Value) -> Option<(String, usize)> {
        let file = self.files.get(decl["id"].as_str()?)?;
        let (offset, _) = location(&decl["loc"])?;
        let before = self.texts.get(file)?.get(..offset)?;
        Some((file.clone(), before.matches('\n').count() + 1))
    }

    /// A function's or field's comment: on its own line after it (`int x; // the x`),
    /// or else a doc comment (`///`, `/** */`) right above it.
    fn of_function(&self, decl: &Value) -> Vec<String> {
        let trailing = self.after(decl);
        if trailing.is_empty() {
            self.above(decl, false)
        } else {
            trailing
        }
    }

    /// The comment after a declaration, on the same line.
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
    fn above(&self, decl: &Value, any: bool) -> Vec<String> {
        let Some((text, start, _)) = self.range(decl) else {
            return Vec::new();
        };
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let before = text[..line_start].trim_end_matches(['\n', '\r']);
        let last = before.rsplit('\n').next().unwrap_or_default().trim();

        let one_line_block = |l: &str| l.starts_with("/*") && l.ends_with("*/");
        if last.ends_with("*/") && !one_line_block(last) {
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

        // `//` lines, or one-line `/* */` comments stacked into a paragraph.
        let mut lines: Vec<&str> = Vec::new();
        for line in before.lines().rev() {
            let trimmed = line.trim();
            if !(trimmed.starts_with("//") || one_line_block(trimmed)) {
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
    fn write(&self, lines: &[String], indent: &str) -> String {
        lines
            .iter()
            .map(|line| format!("{indent}/// {line}").trim_end().to_string() + "\n")
            .collect()
    }
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
    // A ` * ` margin, when every line has one.
    if lines
        .iter()
        .skip(1)
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with('*'))
    {
        lines = lines
            .iter()
            .enumerate()
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
    let indent = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
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
fn comment_lines(line: &str) -> Vec<String> {
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

#[derive(Default)]
struct Converter {
    /// Struct tag → its field declarations, from the header.
    records: HashMap<String, Vec<Value>>,
    typedefs: HashMap<String, String>,
    /// The structs used so far, converted: som name → fields.
    structs: BTreeMap<String, Vec<(String, String)>>,
    /// Each converted struct's C tag, by som name: `MaDevice` → `ma_device`.
    tags: HashMap<String, String>,
    /// Structs that can't be converted, and why.
    unsupported: HashMap<String, String>,
}

impl Converter {
    /// A converter knowing the structs and typedefs among `decls`.
    fn of(decls: &[Value]) -> Converter {
        let mut c = Converter::default();
        for decl in decls {
            match decl["kind"].as_str() {
                Some("RecordDecl") if decl["tagUsed"] == "struct" => {
                    if let (Some(name), Some(fields)) =
                        (decl["name"].as_str(), decl["inner"].as_array())
                    {
                        c.records.insert(name.to_string(), fields.clone());
                    }
                }
                Some("TypedefDecl") => {
                    if let (Some(name), Some(ty)) =
                        (decl["name"].as_str(), decl["type"]["qualType"].as_str())
                    {
                        c.typedefs.insert(name.to_string(), ty.to_string());
                    }
                }
                _ => {}
            }
        }
        c
    }

    /// A function's som signature.
    fn signature(&mut self, decl: &Value) -> Result<Signature, String> {
        let signature = decl["type"]["qualType"].as_str().unwrap_or_default();
        let returns = self.ty(return_type(signature).ok_or("its return type is too complex")?)?;
        let mut params = Vec::new();
        for (i, param) in decl["inner"].as_array().into_iter().flatten().enumerate() {
            if param["kind"] != "ParmVarDecl" {
                continue;
            }
            let ty = self.param_ty(type_text(&param["type"]))?;
            let name = param["name"]
                .as_str()
                .map_or(format!("arg{i}"), |n| identifier(&snake_case(n)));
            params.push((name, ty));
        }
        Ok(Signature {
            params,
            variadic: decl["variadic"] == true,
            returns,
        })
    }

    /// `fn name(a: i32, b: &str) -> f32;`
    fn function(&mut self, decl: &Value, name: &str) -> Result<String, String> {
        Ok(format!("{};", self.signature(decl)?.text(name)))
    }

    /// The som type for a parameter: like [`Converter::ty`], except that a pointer to
    /// a struct, a number or another pointer is a reference, `&mut` unless what it
    /// points to is `const`. Return values and fields stay `*`: C can hand back NULL,
    /// or memory the caller now owns.
    fn param_ty(&mut self, text: &str) -> Result<String, String> {
        // `int *const`: a pointer that can't be changed, to something that can.
        let text = text.trim();
        let text = (text.strip_suffix("const").map(str::trim_end))
            .filter(|t| t.ends_with('*'))
            .unwrap_or(text);
        let Some(pointee) = text.strip_suffix('*').map(str::trim) else {
            return self.ty(text);
        };
        if self.ty(text)? == "&str" {
            return Ok("&str".into());
        }
        let constant = pointee.starts_with("const ") || pointee.ends_with(" const");
        match self.ty(pointee) {
            Ok(to) if to != "()" && !to.starts_with('&') && !pointee.contains('(') => {
                Ok(format!("&{}{to}", if constant { "" } else { "mut " }))
            }
            // `void *`, an opaque struct, a function pointer, `const char *`.
            _ => self.ty(text),
        }
    }

    /// The som type for a C type, importing any struct it names.
    fn ty(&mut self, text: &str) -> Result<String, String> {
        let text = text.trim();
        let constant = text.starts_with("const ") || text.contains(" const");
        let text = text
            .trim_start_matches("const ")
            .trim_start_matches("volatile ")
            .trim_end_matches(" const")
            .trim();

        if text.contains('[') {
            return Err("it uses an array".into());
        }
        if text.contains('(') {
            return Ok("*".into()); // a function pointer
        }
        if let Some(pointee) = text.strip_suffix('*') {
            let pointee = pointee.trim().trim_start_matches("const ").trim();
            return Ok(if constant && pointee == "char" {
                "&str"
            } else {
                "*"
            }
            .into());
        }
        Ok(match text {
            "void" => "()",
            "_Bool" | "bool" => "bool",
            "char" | "signed char" => "i8",
            "unsigned char" => "u8",
            "short" | "short int" | "signed short" => "i16",
            "unsigned short" | "unsigned short int" => "u16",
            "int" | "signed" | "signed int" => "i32",
            "unsigned" | "unsigned int" => "u32",
            "long" | "long int" | "long long" | "long long int" | "signed long" => "i64",
            "unsigned long"
            | "unsigned long int"
            | "unsigned long long"
            | "unsigned long long int" => "u64",
            "float" => "f32",
            "double" => "f64",
            "long double" => return Err("it uses `long double`".into()),
            _ => {
                if let Some(tag) = text.strip_prefix("struct ") {
                    return self.import_struct(tag);
                }
                if text.starts_with("enum ") {
                    return Ok("i32".into());
                }
                if text.starts_with("union ") {
                    return Err("it uses a union".into());
                }
                return match self.typedefs.get(text).cloned() {
                    Some(underlying) if underlying != text => self.ty(&underlying),
                    _ => Err(format!("it uses `{text}`, which som can't express yet")),
                };
            }
        }
        .into())
    }

    /// A struct, under its som name: `struct ma_device` is `MaDevice`.
    fn import_struct(&mut self, tag: &str) -> Result<String, String> {
        let name = pascal_case(tag);
        match self.tags.get(&name) {
            Some(other) if other == tag => return Ok(name),
            Some(other) => {
                return Err(format!(
                    "it uses `{tag}`, which in som would be called `{name}`, like `{other}`"
                ));
            }
            None => {}
        }
        if let Some(reason) = self.unsupported.get(tag) {
            return Err(reason.clone());
        }
        let Some(fields) = self.records.get(tag).cloned() else {
            return Err(format!(
                "it uses `struct {tag}`, which the header doesn't define"
            ));
        };
        let mut converted = Vec::new();
        for field in fields.iter().filter(|f| f["kind"] == "FieldDecl") {
            let name = field["name"].as_str().unwrap_or_default().to_string();
            match self.ty(type_text(&field["type"])) {
                Ok(ty) => converted.push((name, ty)),
                Err(reason) => {
                    let reason = format!(
                        "it uses `{tag}`, whose field `{name}` can't be expressed ({reason})"
                    );
                    self.unsupported.insert(tag.to_string(), reason.clone());
                    return Err(reason);
                }
            }
        }
        self.tags.insert(name.clone(), tag.to_string());
        self.structs.insert(name.clone(), converted);
        Ok(name)
    }
}

/// A type with its typedefs resolved, as clang writes it.
fn type_text(ty: &Value) -> &str {
    ty["desugaredQualType"]
        .as_str()
        .or(ty["qualType"].as_str())
        .unwrap_or_default()
}

/// The return type in a function type like `const char *(int, float)`: everything
/// before the parenthesised parameter list at the end.
fn return_type(signature: &str) -> Option<&str> {
    let signature = signature.trim();
    if !signature.ends_with(')') {
        return None;
    }
    let mut depth = 0;
    for (i, c) in signature.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    let returns = signature[..i].trim();
                    // `void (*(int))(int)`, a function returning a function pointer.
                    return (!returns.ends_with(')')).then_some(returns);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_types() {
        assert_eq!(return_type("void (int, int, const char *)"), Some("void"));
        assert_eq!(return_type("const char *(void)"), Some("const char *"));
        assert_eq!(return_type("Color (int, int (*)(int))"), Some("Color"));
    }

    #[test]
    fn types() {
        let mut c = Converter::default();
        c.typedefs.insert("size_t".into(), "unsigned long".into());
        assert_eq!(c.ty("const char *").unwrap(), "&str");
        assert_eq!(c.ty("char *").unwrap(), "*");
        assert_eq!(c.ty("size_t").unwrap(), "u64");
        assert_eq!(c.ty("void (*)(int)").unwrap(), "*");
        assert_eq!(c.ty("enum Mode").unwrap(), "i32");
        assert!(c.ty("float [4]").is_err());
    }

    #[test]
    fn pointer_parameters_are_references() {
        let mut c = Converter::default();
        assert_eq!(c.param_ty("const float *").unwrap(), "&f32");
        assert_eq!(c.param_ty("int *").unwrap(), "&mut i32");
        assert_eq!(c.param_ty("int *const").unwrap(), "&mut i32");
        assert_eq!(c.param_ty("struct opaque **").unwrap(), "&mut *");
        assert_eq!(c.param_ty("const char *").unwrap(), "&str");
        assert_eq!(c.param_ty("const char **").unwrap(), "*");
        assert_eq!(c.param_ty("void *").unwrap(), "*");
        assert_eq!(c.param_ty("struct opaque *").unwrap(), "*");
        assert_eq!(c.param_ty("void (*)(int)").unwrap(), "*");
        assert_eq!(c.param_ty("int").unwrap(), "i32");
    }
}

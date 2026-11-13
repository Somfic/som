//! Finds the libraries `extern "name" { … }` blocks link against, and checks that
//! each declared function is really in them, before anything is compiled.
//!
//! `extern "raylib"` means `libraylib.dylib` or `libraylib.a` (`.so` on Linux),
//! looked for in the program's own directory, then the usual system places,
//! then `LIBRARY_PATH`. A name that looks like a path (`"libs/libfoo.dylib"`)
//! is used as written, relative to the program. Function names are read from
//! the library's symbol table, so nothing has to be compiled or linked.
//!
//! Plain `extern { … }` blocks link against libc, which isn't a file on every
//! system (macOS keeps it in a shared cache), so those aren't checked.

use object::read::archive::ArchiveFile;
use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::{BinaryFormat, FileKind, Object, ObjectSymbol};
use som_ast::{Ast, Item, Span};
use som_base::diagnostics::{closest_match, edit_distance, snake_case};
use som_base::message as m;
use som_base::{Diagnostic, Label, Message, msg};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum LibraryError {
    LibraryNotFound {
        library: String,
        span: Span,
        files: Vec<String>,
        searched: Vec<PathBuf>,
        /// A library in those places with a similar name.
        suggestion: Option<String>,
    },
    UnreadableLibrary {
        path: PathBuf,
        message: String,
        span: Span,
    },
    /// A hand-written declaration that disagrees with the library's header.
    HeaderMismatch {
        function: String,
        header: String,
        /// The header's declaration, in som.
        declared_there: String,
        problem: Message,
        span: Span,
    },
    /// A function declared in an `extern` block that its library doesn't have.
    NotInLibrary {
        library: String,
        file: PathBuf,
        function: String,
        span: Span,
        suggestion: Option<String>,
    },
}

impl LibraryError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            LibraryError::LibraryNotFound {
                library,
                span,
                files,
                searched,
                suggestion,
            } => {
                let mut looked = msg!["looked for "];
                for (i, file) in files.iter().enumerate() {
                    looked.push(if i == 0 {
                        ""
                    } else if i + 1 == files.len() {
                        " and "
                    } else {
                        ", "
                    });
                    looked.push(m::path(file));
                }
                looked.push(" in ");
                for (i, dir) in searched.iter().enumerate() {
                    looked.push(if i == 0 { "" } else { ", " });
                    looked.push(m::path(dir.display()));
                }
                let d = Diagnostic::error(msg!["cannot find library ", m::path(library)])
                    .with_label(Label::primary(span.clone(), "not found"));
                match suggestion {
                    Some(close) => d.with_hint(msg!["did you mean ", m::path(close), "?"]),
                    None => d
                        .with_hint(looked)
                        .with_hint("install it, or put it next to your program"),
                }
            }
            LibraryError::UnreadableLibrary {
                path,
                message,
                span,
            } => Diagnostic::error(msg![
                "cannot read library ",
                m::path(path.display()),
                format!(": {message}")
            ])
            .with_label(Label::primary(span.clone(), "linked from here")),
            LibraryError::HeaderMismatch {
                function,
                header,
                declared_there,
                problem,
                span,
            } => Diagnostic::error(msg![m::func(function), " doesn't match ", m::path(header)])
                .with_label(Label::primary(span.clone(), problem.clone()))
                .with_hint(msg![
                    m::path(header),
                    " declares it as ",
                    m::code(declared_there)
                ]),
            LibraryError::NotInLibrary {
                library,
                file,
                function,
                span,
                suggestion,
            } => {
                let d = Diagnostic::error(msg![
                    m::path(library),
                    " has no function ",
                    m::func(function)
                ])
                .with_label(Label::primary(
                    span.clone(),
                    msg!["not in ", m::path(file.display())],
                ));
                match suggestion {
                    Some(close) => d.with_hint(msg!["did you mean ", m::func(close), "?"]),
                    None => d,
                }
            }
        }
    }
}

/// Checks every `extern "library"` block in the program, looking in `extra` directories
/// as well. Returns the library files found, for linking, and the problems.
pub fn check(ast: &mut Ast, extra: &[PathBuf]) -> (Vec<PathBuf>, Vec<LibraryError>) {
    let root = ast.modules[0].path.clone();
    let mut found: Vec<PathBuf> = Vec::new();
    let mut symbols: HashMap<PathBuf, Result<Symbols, String>> = HashMap::new();
    let mut errors = Vec::new();
    let mut link_names = Vec::new();

    for item in ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let (Some(library), Some(span)) = (&block.library, &block.library_span) else {
            continue;
        };

        let path = match find(library, &root, extra) {
            Ok(path) => path,
            Err(searched) => {
                let suggestion = if is_path(library) {
                    None
                } else {
                    similar_library(library, &searched)
                };
                errors.push(LibraryError::LibraryNotFound {
                    library: library.clone(),
                    span: span.clone(),
                    files: file_names(library),
                    searched,
                    suggestion,
                });
                continue;
            }
        };
        if !found.contains(&path) {
            found.push(path.clone());
        }
        let exported = match symbols
            .entry(path.clone())
            .or_insert_with(|| read_symbols(&path).map(Symbols::new))
        {
            Ok(exported) => exported,
            Err(message) => {
                errors.push(LibraryError::UnreadableLibrary {
                    path,
                    message: message.clone(),
                    span: span.clone(),
                });
                continue;
            }
        };

        for &func in &block.funcs {
            let name = &ast.funcs[func].name;
            match exported.symbol_for(&name.name) {
                Some(symbol) if symbol != &*name.name => {
                    link_names.push((func, symbol.to_string()))
                }
                Some(_) => {}
                None => errors.push(LibraryError::NotInLibrary {
                    library: library.clone(),
                    file: path.clone(),
                    function: name.name.to_string(),
                    span: name.span.clone(),
                    suggestion: closest_match(&name.name, &exported.snake, 2).map(str::to_string),
                }),
            }
        }
    }

    for (func, symbol) in link_names {
        ast.funcs.get_mut(func).link_name = Some(symbol.into());
    }
    (found, errors)
}

/// A library's exported names, and the snake_case name som code uses for each.
struct Symbols {
    /// Sorted.
    names: Vec<String>,
    /// snake_case name → the symbol, for symbols whose snake_case name differs.
    by_snake: HashMap<String, String>,
    /// Every name som code could use, sorted: for suggestions.
    snake: Vec<String>,
}

impl Symbols {
    fn new(names: Vec<String>) -> Symbols {
        let mut by_snake = HashMap::new();
        for name in &names {
            let snake = snake_case(name);
            if snake != *name {
                // Of two symbols with the same snake_case name, the first (sorted) wins.
                by_snake.entry(snake).or_insert_with(|| name.clone());
            }
        }
        let mut snake: Vec<String> = names.iter().map(|n| snake_case(n)).collect();
        snake.sort();
        snake.dedup();
        Symbols {
            names,
            by_snake,
            snake,
        }
    }

    /// The symbol a declared name links to: the symbol of that exact name, or the one
    /// whose snake_case name it is (`init_window` is `InitWindow`). A trailing `_`, added
    /// when the name is a som keyword, is ignored.
    fn symbol_for(&self, declared: &str) -> Option<&str> {
        [declared, declared.strip_suffix('_').unwrap_or(declared)]
            .into_iter()
            .find_map(
                |name| match self.names.binary_search_by(|n| n.as_str().cmp(name)) {
                    Ok(i) => Some(self.names[i].as_str()),
                    Err(_) => self.by_snake.get(name).map(String::as_str),
                },
            )
    }
}

/// Whether a library is written as a path rather than a name.
fn is_path(library: &str) -> bool {
    library.contains('/')
        || [".so", ".dylib", ".a", ".dll", ".lib"]
            .iter()
            .any(|ext| library.ends_with(ext))
}

/// The files a library name can be: `raylib` is `libraylib.dylib` or `libraylib.a`.
fn file_names(library: &str) -> Vec<String> {
    let extensions: &[&str] = if cfg!(target_os = "macos") {
        &["dylib", "a"]
    } else if cfg!(windows) {
        &["dll", "lib"]
    } else {
        &["so", "a"]
    };
    extensions
        .iter()
        .map(|ext| format!("lib{library}.{ext}"))
        .collect()
}

/// Where libraries are looked for: the program's directory, then `extra` (from `-L`),
/// then the system's.
pub(crate) fn search_dirs(root: &Path, extra: &[PathBuf]) -> Vec<PathBuf> {
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(extra.iter().cloned());
    if cfg!(target_os = "macos") {
        dirs.push("/opt/homebrew/lib".into());
    }
    dirs.extend(["/usr/local/lib".into(), "/usr/lib".into()]);
    if let Some(paths) = std::env::var_os("LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    dirs
}

/// The name of a library in `dirs` that's spelled almost like `library`. Of equally
/// close names, the one in the earliest directory wins, so the program's own come first.
fn similar_library(library: &str, dirs: &[PathBuf]) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let file = entry.file_name().into_string().ok()?;
                let (stem, _) = file.strip_prefix("lib")?.split_once('.')?;
                file_names(stem).contains(&file).then(|| stem.to_string())
            })
            .collect();
        names.sort();
        for name in names {
            let distance = edit_distance(library, &name);
            if distance <= 2 && best.as_ref().is_none_or(|(d, _)| distance < *d) {
                best = Some((distance, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

/// The file `library` refers to, or the directories that were searched for it.
pub(crate) fn find(library: &str, root: &Path, extra: &[PathBuf]) -> Result<PathBuf, Vec<PathBuf>> {
    if is_path(library) {
        let path = root.join(library);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(vec![root.to_path_buf()])
        };
    }
    let dirs = search_dirs(root, extra);
    for dir in &dirs {
        for file in file_names(library) {
            let path = dir.join(file);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(dirs)
}

/// The functions a library exports, sorted.
pub(crate) fn read_symbols(path: &Path) -> Result<Vec<String>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut names = HashSet::new();
    collect_symbols(&data, &mut names)?;
    let mut names: Vec<String> = names.into_iter().collect();
    names.sort();
    Ok(names)
}

fn collect_symbols(data: &[u8], names: &mut HashSet<String>) -> Result<(), String> {
    let err = |e: object::Error| e.to_string();
    match FileKind::parse(data).map_err(err)? {
        // A static library: every object file in it.
        FileKind::Archive => {
            let archive = ArchiveFile::parse(data).map_err(err)?;
            for member in archive.members() {
                let member = member.map_err(err)?;
                // Members that aren't object files, like the archive's index, are skipped.
                let _ = collect_symbols(member.data(data).map_err(err)?, names);
            }
        }
        // A universal binary: one library per architecture.
        FileKind::MachOFat32 => {
            for arch in MachOFatFile32::parse(data).map_err(err)?.arches() {
                collect_symbols(arch.data(data).map_err(err)?, names)?;
            }
        }
        FileKind::MachOFat64 => {
            for arch in MachOFatFile64::parse(data).map_err(err)?.arches() {
                collect_symbols(arch.data(data).map_err(err)?, names)?;
            }
        }
        _ => {
            let file = object::File::parse(data).map_err(err)?;
            // Mach-O puts an `_` in front of every C name.
            let macho = file.format() == BinaryFormat::MachO;
            let mut add = |name: &str| {
                let name = if macho {
                    name.strip_prefix('_').unwrap_or(name)
                } else {
                    name
                };
                names.insert(name.to_string());
            };
            if let Ok(exports) = file.exports() {
                for export in exports {
                    if let Ok(name) = std::str::from_utf8(export.name()) {
                        add(name);
                    }
                }
            }
            for symbol in file.dynamic_symbols().chain(file.symbols()) {
                if symbol.is_global()
                    && symbol.is_definition()
                    && let Ok(name) = symbol.name()
                {
                    add(name);
                }
            }
        }
    }
    Ok(())
}

//! Function names are read from each library's symbol table, so nothing has to be
//! compiled or linked to check them.
//!
//! Plain `extern { … }` blocks link against libc, which isn't a file on every
//! system (macOS keeps it in a shared cache), so those aren't checked.

use object::read::archive::ArchiveFile;
use object::read::macho::{FatArch, MachOFatFile32, MachOFatFile64};
use object::{BinaryFormat, FileKind, Object, ObjectSymbol};
use som_ast::{Ast, Item, Span};
use som_base::diagnostics::{closest_match, edit_distance, snake_case};
use som_base::message as m;
use som_base::{Diagnostic, Fix, Label, Message, msg};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::toolchain::{self, Target, Toolchain};

#[derive(Debug)]
pub enum LibraryError {
    LibraryNotFound {
        library: String,
        span: Span,
        files: Vec<String>,
        searched: Vec<PathBuf>,
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
    NotInLibrary {
        library: String,
        file: PathBuf,
        function: String,
        span: Span,
        suggestion: Option<String>,
    },
}

#[derive(Debug)]
pub enum LibraryWarning {
    /// Hand-written `extern "raylib" { … }` declarations that are exactly what
    /// `extern "raylib";` imports from the header.
    DeclaredInHeader {
        library: String,
        header: String,
        /// Where the library's name is written.
        span: Span,
        block: Span,
        functions: Vec<SameAsHeader>,
        /// Whether that's every declaration in the block.
        all: bool,
    },
    /// A hand-written struct with the same name and fields as one the header declares.
    StructInHeader {
        name: String,
        library: String,
        header: String,
        /// What it's called when imported: `raylib::Color`.
        imported: String,
        span: Span,
        declaration: Span,
    },
}

#[derive(Debug)]
pub struct SameAsHeader {
    pub name: String,
    pub span: Span,
    pub declaration: Span,
    /// What it's called when imported: `raylib::init_window`.
    pub imported: String,
}

fn generates(library: &str, header: &str) -> Message {
    msg![
        "when a library's header can be found, ",
        m::code(format!("extern \"{library}\";")),
        " generates its functions, structs, enums and constants from ",
        m::path(header),
        ", so none of them need writing by hand"
    ]
}

/// `span` grown to the whole lines it's on, with a `pub` before it and the `///`
/// comments above it, so removing it leaves no gap. Left as it is when it shares a
/// line with something else.
fn whole_lines(span: &Span) -> Span {
    let text = span.source.content();
    let (mut start, mut end) = (span.start_offset, span.start_offset + span.length);
    let first = line_start_of(text, start);
    let after = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
    let before = text[first..start].trim();
    if !(before.is_empty() || before == "pub") || !text[end..after].trim().is_empty() {
        return span.clone();
    }
    (start, end) = (first, after);
    while start > 0 {
        let above = line_start_of(text, start - 1);
        if !text[above..start].trim_start().starts_with("///") {
            break;
        }
        start = above;
    }
    Span::from_range(start..end, span.source.clone())
}

fn line_start_of(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |i| i + 1)
}

impl LibraryWarning {
    pub fn span(&self) -> &Span {
        match self {
            LibraryWarning::DeclaredInHeader { span, .. }
            | LibraryWarning::StructInHeader { span, .. } => span,
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            LibraryWarning::DeclaredInHeader {
                library,
                header,
                span,
                block,
                functions,
                all,
            } => {
                let import = format!("extern \"{library}\";");
                let d = if *all {
                    Diagnostic::warning(msg![
                        m::path(header),
                        " already declares everything in this block"
                    ])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![m::code(&import), " imports all of it"],
                    ))
                    .with_fix(Fix::replace(
                        msg!["replace the block with ", m::code(&import)],
                        block.clone(),
                        &import,
                    ))
                } else {
                    let mut d = Diagnostic::warning(msg![
                        m::path(header),
                        format!(
                            " already declares {} of the functions in this block",
                            functions.len()
                        ),
                    ])
                    .with_label(Label::primary(
                        span.clone(),
                        msg![m::code(&import), " imports them"],
                    ));
                    for f in functions {
                        d = d
                            .with_label(Label::secondary(f.span.clone(), "the same as the header"));
                    }
                    let text = block.source.content();
                    let line = line_start_of(text, block.start_offset);
                    let indent = &text[line..block.start_offset];
                    let indent: String = indent.chars().take_while(|c| c.is_whitespace()).collect();
                    let mut edits = vec![(
                        Span::from_range(line..line, block.source.clone()),
                        format!("{indent}{import}\n"),
                    )];
                    edits.extend(
                        (functions.iter()).map(|f| (whole_lines(&f.declaration), String::new())),
                    );
                    d.with_fix(Fix {
                        title: msg!["add ", m::code(&import), " and remove what it imports"],
                        edits,
                    })
                };
                let d = d.with_hint(generates(library, header));
                match functions
                    .iter()
                    .find(|f| f.imported.rsplit("::").next() != Some(f.name.as_str()))
                {
                    Some(f) => d.with_hint(msg![
                        "imported, they're named in snake_case: ",
                        m::func(&f.name),
                        " is ",
                        m::func(&f.imported)
                    ]),
                    None => d,
                }
            }
            LibraryWarning::StructInHeader {
                name,
                library,
                header,
                imported,
                span,
                declaration,
            } => Diagnostic::warning(msg![
                m::struct_name(name),
                " is already declared in ",
                m::path(header)
            ])
            .with_label(Label::primary(
                span.clone(),
                "the same fields as the header's",
            ))
            .with_hint(generates(library, header))
            .with_hint(msg![
                "imported, it's ",
                m::struct_name(imported),
                ", which copies like plain data"
            ])
            .with_fix(Fix {
                title: msg!["remove ", m::struct_name(name)],
                edits: vec![(whole_lines(declaration), String::new())],
            }),
        }
    }
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

/// Checks every `extern "library"` block in the program. Returns the library files found,
/// for linking, and the problems.
pub fn check(ast: &mut Ast, toolchain: &Toolchain) -> (Vec<PathBuf>, Vec<LibraryError>) {
    let root = ast.modules[0].path.clone();
    let mut found: Vec<PathBuf> = Vec::new();
    let mut symbols: HashMap<PathBuf, Result<Symbols, String>> = HashMap::new();
    let mut errors = Vec::new();
    let mut link_names = Vec::new();
    let mut linked_from = Vec::new();

    for item in ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let (Some(library), Some(span)) = (&block.library, &block.library_span) else {
            continue;
        };
        // C's are always there; GLSL's are the GPU's own.
        if library == crate::headers::LIBC || library == som_lang::GL_LIBRARY {
            continue;
        }

        let path = match find(library, &root, toolchain) {
            Ok(path) => path,
            Err(searched) => {
                let suggestion = if is_path(library) {
                    None
                } else {
                    similar_library(library, &searched, &toolchain.target)
                };
                errors.push(LibraryError::LibraryNotFound {
                    library: library.clone(),
                    span: span.clone(),
                    files: toolchain.target.library_files(library),
                    searched,
                    suggestion,
                });
                continue;
            }
        };
        if !found.contains(&path) {
            found.push(path.clone());
        }
        linked_from.extend(block.funcs.iter().map(|&func| (func, path.clone())));
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
    for (func, path) in linked_from {
        ast.funcs.get_mut(func).library = Some(path);
    }
    (found, errors)
}

struct Symbols {
    /// Sorted, for binary search.
    names: Vec<String>,
    /// Only symbols whose snake_case name differs.
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

/// `"libs/libfoo.dylib"`, used as written, relative to the program.
fn is_path(library: &str) -> bool {
    library.contains('/')
        || (toolchain::LIBRARY_EXTENSIONS.iter()).any(|ext| library.ends_with(&format!(".{ext}")))
}

/// The program's directory, then the toolchain's (from `-L`), then the system's, then
/// `LIBRARY_PATH` and Nix's `NIX_LDFLAGS`, which are only for the system som runs on.
pub(crate) fn search_dirs(root: &Path, toolchain: &Toolchain) -> Vec<PathBuf> {
    let mut dirs = vec![toolchain::program_dir(root).to_path_buf()];
    dirs.extend(toolchain.library_dirs.iter().cloned());
    dirs.extend(toolchain.target.system_library_dirs());
    if toolchain.target.is_cross() {
        return dirs;
    }
    if let Some(paths) = std::env::var_os("LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }
    if let Ok(flags) = std::env::var("NIX_LDFLAGS") {
        for dir in flag_dirs(&flags, &["-L"]) {
            if !dirs.contains(&dir) {
                dirs.push(dir);
            }
        }
    }
    dirs
}

/// The directories given with any of `flags` in a compiler or linker command line, like
/// the `-L` ones in `NIX_LDFLAGS`, where a Nix shell puts its packages' libraries instead
/// of `LIBRARY_PATH`. Both `-L/dir` and `-L /dir` are understood.
pub(crate) fn flag_dirs(line: &str, flags: &[&str]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut words = line.split_whitespace();
    while let Some(word) = words.next() {
        let dir = match flags.iter().find_map(|flag| word.strip_prefix(flag)) {
            Some("") => words.next(),
            other => other,
        };
        if let Some(dir) = dir {
            dirs.push(PathBuf::from(dir));
        }
    }
    dirs
}

/// The name of a library in `dirs` that's spelled almost like `library`. Of equally
/// close names, the one in the earliest directory wins, so the program's own come first.
fn similar_library(library: &str, dirs: &[PathBuf], target: &Target) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let file = entry.file_name().into_string().ok()?;
                let name = toolchain::library_name(&file)?;
                (target.library_files(name).contains(&file)).then(|| name.to_string())
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
pub(crate) fn find(
    library: &str,
    root: &Path,
    toolchain: &Toolchain,
) -> Result<PathBuf, Vec<PathBuf>> {
    if is_path(library) {
        let path = root.join(library);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(vec![root.to_path_buf()])
        };
    }
    let dirs = search_dirs(root, toolchain);
    for dir in &dirs {
        for file in toolchain.target.library_files(library) {
            let path = dir.join(file);
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(dirs)
}

/// Sorted.
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
        FileKind::Archive => {
            let archive = ArchiveFile::parse(data).map_err(err)?;
            for member in archive.members() {
                let member = member.map_err(err)?;
                // Members that aren't object files, like the archive's index, are skipped.
                let _ = collect_symbols(member.data(data).map_err(err)?, names);
            }
        }
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

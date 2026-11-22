//! `@if WINDOWS { … }`, or `@if OS == Os::Windows { … }`: code for only some systems.
//!
//! The system a program is built for is known while loading, so these are decided before
//! names are resolved: the branch for other systems isn't resolved, checked or compiled,
//! and can name what only exists there, like `c::_putenv_s`. Every C library function is
//! still checked against each system its code is for, so `c::fork` outside such an `@if`
//! is an error on any system.
//!
//! `@if GPU { … }`: code for shaders. The GPU is a system too, but it's not decided while
//! loading: a shader is compiled for the GPU and, as an ordinary function, for the CPU, in
//! the same build. Both branches of `@if GPU` are resolved and checked, and lowering takes
//! the one for each copy. WGSL's built-ins (`extern "wgsl"`, in `som::gpu`) are only on the GPU, C's only on the
//! others; on the GPU, an `@if` on the system takes the target's branch.

use som_ast::{Ast, BinOp, Child, Expr, Func, Id, Item, Path, Stmt, Visibility};
use som_base::message::{self as m, Message};
use som_base::{Diagnostic, Label, Source, Span, msg};
use som_resolve::{DefKind, Names};
use std::collections::{HashMap, HashSet};
use std::path::Path as FilePath;
use std::sync::{Arc, OnceLock};

use crate::toolchain::{self, Os, Target};

/// `som::OS`, the system the program is compiled for, written into the standard library's
/// root module for each build.
const OS_CONST: &str = "OS";
const OS_ENUM: &str = "Os";

/// The source of `som::OS` and `som::Os` for `target`.
pub fn target_source(target: &Target) -> Source {
    let variants: Vec<&str> = Os::ALL.iter().map(|os| os.variant()).collect();
    let mut text = format!(
        "/// The systems a program can be compiled for.\n\
         pub enum {OS_ENUM} {{ {} }}\n\n\
         /// The system this program is compiled for.\n\
         pub @const {OS_CONST}: {OS_ENUM} = {OS_ENUM}::{};\n",
        variants.join(", "),
        target.os().variant(),
    );
    for os in Os::ALL {
        text.push_str(&format!(
            "\n/// Whether this program is compiled for {}.\npub @const {}: bool = {};\n",
            os.name(),
            os.constant(),
            os == target.os(),
        ));
    }
    text.push_str(&format!(
        "\n/// Whether this code is compiled for the GPU: in a shader, or what one calls. Ask it \
         with `@if {gpu} {{ … }}`, whose branches are both checked.\npub @const {gpu}: bool = false;\n",
        gpu = som_lang::GPU_CONST,
    ));
    Source::from_raw_at(text, "<target>")
}

/// Some of the systems, as a set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Systems(u8);

impl Systems {
    /// The systems a program is compiled for: every OS.
    pub const OSES: Systems = Systems((1 << Os::ALL.len()) - 1);
    /// Where shaders run.
    pub const GPU: Systems = Systems(1 << Os::ALL.len());
    pub const ALL: Systems = Systems(Systems::OSES.0 | Systems::GPU.0);
    const NONE: Systems = Systems(0);

    fn of(os: Os) -> Systems {
        Systems(1 << Os::ALL.iter().position(|&o| o == os).expect("one of all"))
    }

    pub fn contains(self, os: Os) -> bool {
        self.0 & Systems::of(os).0 != 0
    }

    fn has(self, other: Systems) -> bool {
        self.0 & other.0 == other.0
    }

    /// Each system in it, alone.
    fn each(self) -> impl Iterator<Item = Systems> {
        (Os::ALL.into_iter().map(Systems::of))
            .chain(std::iter::once(Systems::GPU))
            .filter(move |&one| self.has(one))
    }

    fn and(self, other: Systems) -> Systems {
        Systems(self.0 & other.0)
    }

    fn or(self, other: Systems) -> Systems {
        Systems(self.0 | other.0)
    }

    fn not(self) -> Systems {
        Systems(!self.0 & Systems::ALL.0)
    }

    fn without(self, other: Systems) -> Systems {
        self.and(other.not())
    }

    fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn oses(self) -> impl Iterator<Item = Os> {
        Os::ALL.into_iter().filter(move |&os| self.contains(os))
    }

    /// `linux and windows`, `the GPU`.
    fn names(self) -> Message {
        let mut parts: Vec<m::Part> = self.oses().map(|os| m::code(os.name())).collect();
        if self.has(Systems::GPU) {
            parts.push("the GPU".into());
        }
        Message::list(parts, " and ")
    }

    /// The condition that holds on exactly these: `!WINDOWS`, `LINUX or MACOS`, with
    /// `som::` where it's not the standard library's own.
    fn condition(self, in_std: bool) -> String {
        let som = if in_std { "" } else { "som::" };
        let gpu = som_lang::GPU_CONST;
        if self == Systems::GPU {
            return format!("{som}{gpu}");
        }
        if self.has(Systems::OSES) {
            return format!("!{som}{gpu}");
        }
        let this = self.and(Systems::OSES);
        let (not, oses) = match this.not().and(Systems::OSES).oses().count() {
            1 => ("!", this.not().and(Systems::OSES)),
            _ => ("", this),
        };
        let parts: Vec<String> = (oses.oses())
            .map(|os| format!("{not}{som}{}", os.constant()))
            .collect();
        parts.join(if not.is_empty() { " or " } else { " and " })
    }
}

/// The systems `condition` holds on, if it only asks which system it is.
fn systems_of(ast: &Ast, condition: Id<Expr>) -> Option<Systems> {
    match &ast.exprs[condition] {
        Expr::Not(inner) => Some(systems_of(ast, *inner)?.not()),
        Expr::Var(path) => {
            let os = Os::ALL
                .into_iter()
                .find(|os| in_som(path) == [os.constant()])?;
            Some(Systems::of(os))
        }
        Expr::Binary { op, lhs, rhs } => {
            let (lhs, rhs) = (*lhs, *rhs);
            match op {
                BinOp::And => Some(systems_of(ast, lhs)?.and(systems_of(ast, rhs)?)),
                BinOp::Or => Some(systems_of(ast, lhs)?.or(systems_of(ast, rhs)?)),
                BinOp::Equals | BinOp::NotEquals => {
                    let os = (os_variant(ast, rhs).filter(|_| is_os_const(ast, lhs)))
                        .or_else(|| os_variant(ast, lhs).filter(|_| is_os_const(ast, rhs)))?;
                    let on = Systems::of(os);
                    Some(if *op == BinOp::Equals { on } else { on.not() })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// For `@if GPU` and `@if !GPU`: whether the first branch is the GPU's.
fn gpu_condition(ast: &Ast, condition: Id<Expr>) -> Option<bool> {
    match &ast.exprs[condition] {
        Expr::Not(inner) => Some(!gpu_condition(ast, *inner)?),
        Expr::Var(path) => (in_som(path) == [som_lang::GPU_CONST]).then_some(true),
        _ => None,
    }
}

/// The names of a path, without the `som::` in front.
fn in_som(path: &Path) -> Vec<&str> {
    let names: Vec<&str> = path.segments.iter().map(|s| &*s.name).collect();
    match names.split_first() {
        Some((&first, rest)) if first == som_lang::PRELUDE_MODULE => rest.to_vec(),
        _ => names,
    }
}

/// `OS` or `som::OS`.
fn is_os_const(ast: &Ast, expr: Id<Expr>) -> bool {
    matches!(&ast.exprs[expr], Expr::Var(path) if in_som(path) == [OS_CONST])
}

/// The system `Os::Windows` or `som::Os::Windows` names.
fn os_variant(ast: &Ast, expr: Id<Expr>) -> Option<Os> {
    let Expr::Var(path) = &ast.exprs[expr] else {
        return None;
    };
    let [ty, variant] = in_som(path)[..] else {
        return None;
    };
    (ty == OS_ENUM)
        .then(|| Os::ALL.into_iter().find(|os| variant == os.variant()))
        .flatten()
}

/// Decides each statement `@if` that only asks which system it is, for `os`: the branch
/// for other systems is left out of resolving and everything after.
pub fn decide(ast: &mut Ast, os: Os) {
    let gpu_ifs: Vec<(Id<Stmt>, bool)> = (ast.stmts.iter())
        .filter_map(|(id, stmt)| match stmt {
            Stmt::If { condition, .. } if ast.settled.contains(&id) => {
                Some((id, gpu_condition(ast, *condition)?))
            }
            _ => None,
        })
        .collect();
    ast.gpu_ifs.extend(gpu_ifs);
    let decided: Vec<(Id<Stmt>, bool)> = (ast.stmts.iter())
        .filter_map(|(id, stmt)| match stmt {
            Stmt::If { condition, .. } if ast.settled.contains(&id) => {
                Some((id, systems_of(ast, *condition)?.contains(os)))
            }
            _ => None,
        })
        .collect();
    ast.decided.extend(decided);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibcKind {
    Func,
    Struct,
    Const,
}

/// A name the C library has on some system.
#[derive(Debug)]
pub struct LibcDeclaration {
    pub name: String,
    pub kind: LibcKind,
    /// Its line in the first system's file that has it: `pub fn _getpid() -> i32;`.
    pub text: String,
    pub on: Systems,
}

/// What `std/c/<system>.som` declares, for every system, by name.
pub fn libc_declarations() -> &'static [LibcDeclaration] {
    static DECLARATIONS: OnceLock<Vec<LibcDeclaration>> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let mut all: Vec<LibcDeclaration> = Vec::new();
        let mut at: HashMap<String, usize> = HashMap::new();
        for os in Os::ALL {
            let Some(path) = libc_file(os) else { continue };
            let Ok(source) = Source::from_file(&path) else {
                continue;
            };
            let mut ast = Ast::default();
            let module = ast.add_module(som_lang::LIBC_MODULE, path.clone());
            let _ = som_parser::parse_file(Arc::new(source), &mut ast, module);
            let declared = (ast.funcs.iter().map(|(_, f)| (&f.name, LibcKind::Func)))
                .chain(ast.structs.iter().map(|(_, s)| (&s.name, LibcKind::Struct)))
                .chain(ast.consts.iter().map(|(_, c)| (&c.name, LibcKind::Const)));
            for (name, kind) in declared {
                let index = *at.entry(name.name.to_string()).or_insert_with(|| {
                    all.push(LibcDeclaration {
                        name: name.name.to_string(),
                        kind,
                        text: line_of(&name.span),
                        on: Systems::NONE,
                    });
                    all.len() - 1
                });
                all[index].on = all[index].on.or(Systems::of(os));
            }
        }
        all.sort_by(|a, b| a.name.cmp(&b.name));
        all
    })
}

/// The line `span` starts on, trimmed.
fn line_of(span: &Span) -> String {
    let text = span.source.content();
    let start = text[..span.start_offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[span.start_offset..]
        .find('\n')
        .map_or(text.len(), |i| span.start_offset + i);
    text[start..end].trim().to_string()
}

fn libc_names() -> &'static HashMap<String, Systems> {
    static NAMES: OnceLock<HashMap<String, Systems>> = OnceLock::new();
    NAMES.get_or_init(|| {
        (libc_declarations().iter())
            .map(|d| (d.name.clone(), d.on))
            .collect()
    })
}

/// For a C library name not on every system, which it's on, to show with it:
/// `Only on `linux` and `macos`.`
pub fn availability_note(name: &str) -> Option<String> {
    let on = *libc_names().get(name)?;
    (on != Systems::OSES).then(|| format!("Only on {}.", on.names().plain()))
}

/// The short form, for beside a name in a list: `only linux, macos`.
pub fn availability_label(name: &str) -> Option<String> {
    let on = *libc_names().get(name)?;
    let names: Vec<&str> = on.oses().map(Os::name).collect();
    (on != Systems::OSES).then(|| format!("only {}", names.join(", ")))
}

/// Whether `span` is in one of the standard library's C library files.
pub fn in_libc(span: &Span) -> bool {
    is_libc_source(&span.source)
}

/// The files of `som::c` when compiling for `os`: its own, then each other system's with
/// what's already declared blanked out. So every system's names resolve anywhere, the
/// target's own declarations win, and [`infer`] works out where each use can run.
pub fn libc_sources(os: Os) -> Vec<Source> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut sources = Vec::new();
    let others = Os::ALL.into_iter().filter(|&o| o != os);
    for system in std::iter::once(os).chain(others) {
        let Some(path) = libc_file(system) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let source = Source::from_raw_at(text.as_str(), path.clone());
        let mut ast = Ast::default();
        let module = ast.add_module(som_lang::LIBC_MODULE, path.clone());
        let _ = som_parser::parse_file(Arc::new(source.clone()), &mut ast, module);
        let mut repeated = Vec::new();
        for (name, span) in declared(&ast, module) {
            if !taken.insert(name.to_string()) {
                repeated.push(span);
            }
        }
        if repeated.is_empty() {
            sources.push(source);
        } else {
            sources.push(Source::from_raw_at(blank(&text, &repeated), path));
        }
    }
    sources
}

/// The names `module` declares, and where.
fn declared(ast: &Ast, module: usize) -> Vec<(&str, Span)> {
    let mut out = Vec::new();
    for item in &ast.modules[module].items {
        match item {
            Item::Extern(block) => out.extend(
                (block.funcs.iter())
                    .map(|&f| (&*ast.funcs[f].name.name, ast.funcs.span(f).clone())),
            ),
            Item::Struct(id) => {
                out.push((&*ast.structs[*id].name.name, ast.structs.span(*id).clone()))
            }
            Item::Const(id) => {
                out.push((&*ast.consts[*id].name.name, ast.consts.span(*id).clone()))
            }
            _ => {}
        }
    }
    out
}

/// `text` with the whole lines of each span, and the doc comments above them, turned to
/// spaces: lines and offsets stay where they were.
fn blank(text: &str, spans: &[Span]) -> String {
    let mut bytes = text.as_bytes().to_vec();
    for span in spans {
        let mut start = text[..span.start_offset].rfind('\n').map_or(0, |i| i + 1);
        while start > 0 {
            let above = text[..start - 1].rfind('\n').map_or(0, |i| i + 1);
            if !text[above..start].trim_start().starts_with("///") {
                break;
            }
            start = above;
        }
        let end = span.start_offset + span.length;
        let end = text[end..].find('\n').map_or(text.len(), |i| end + i);
        for byte in &mut bytes[start..end] {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    }
    String::from_utf8(bytes).expect("only whole lines are blanked")
}

/// `std/c/<system>.som`.
pub fn libc_file(os: Os) -> Option<std::path::PathBuf> {
    let dir = crate::stdlib::root()?.join(toolchain::LIBC_BINDINGS_DIR);
    Some(dir.join(format!("{}.{}", os.name(), toolchain::SOURCE_EXTENSION)))
}

/// `c::fork` where it's compiled for a system without it.
#[derive(Debug)]
pub struct Unavailable {
    pub name: String,
    pub span: Span,
    /// The systems the code is for that don't have it.
    pub missing: Systems,
    /// Those that do: none for a name no system has.
    pub on: Systems,
    in_std: bool,
}

impl Unavailable {
    pub fn span(&self) -> &Span {
        &self.span
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        let name = format!("c::{}", self.name);
        if self.on.is_empty() {
            return Diagnostic::error(msg!["cannot find ", m::func(&name), " on any system"])
                .with_label(Label::primary(self.span.clone(), "not in the C library"));
        }
        Diagnostic::error(msg![
            m::func(&name),
            " doesn't exist on ",
            self.missing.names()
        ])
        .with_label(Label::primary(
            self.span.clone(),
            msg!["only on ", self.on.names()],
        ))
        .with_hint(msg![
            "use it inside ",
            m::code(format!("@if {} {{ … }}", self.on.condition(self.in_std))),
            ", with something else for the other systems"
        ])
    }
}

/// Which systems a program is checked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checking {
    /// The one it's compiled for: what it can't do there is an error.
    Target(Os),
    /// All of them, as an editor does: what some can't do is a warning.
    EverySystem,
}

/// Each use of a C library name in a branch left out for this target, which is never
/// resolved, so [`infer`] can't see it: against the systems the branch is for. Compiling
/// for one system, that's only names no system has.
pub fn check_left_out(ast: &Ast, checking: Checking) -> Vec<Unavailable> {
    let branches = branches(ast);
    let names = libc_names();
    let mut errors: Vec<Unavailable> = Vec::new();
    for (id, expr) in ast.exprs.iter() {
        let path = match expr {
            Expr::Var(path) | Expr::Call { callee: path, .. } => path,
            _ => continue,
        };
        let Some(name) = libc_name(path) else {
            continue;
        };
        let span = ast.exprs.span(id);
        if is_libc_source(&span.source) {
            continue;
        }
        let around: Vec<&Branch> = (branches.iter())
            .filter(|b| contains(&b.within, span))
            .collect();
        let compiled_for = (around.iter()).fold(Systems::OSES, |on, b| on.and(b.systems));
        let left_out = around.iter().any(|b| b.left_out);
        let on = names.get(name).copied().unwrap_or(Systems::NONE);
        let missing = compiled_for.without(on);
        let wanted = checking == Checking::EverySystem || on.is_empty();
        if !left_out || !wanted || missing.is_empty() || compiled_for.is_empty() {
            continue;
        }
        let name_span = &path.name().span;
        if errors.iter().any(|e| e.span == *name_span) {
            continue;
        }
        errors.push(Unavailable {
            name: name.to_string(),
            span: name_span.clone(),
            missing,
            on,
            in_std: crate::stdlib::contains(FilePath::new(span.source.identifier())),
        });
    }
    errors
}

/// `X` of `c::X` and `som::c::X`.
fn libc_name(path: &Path) -> Option<&str> {
    let names: Vec<&str> = path.segments.iter().map(|s| &*s.name).collect();
    match names[..] {
        ["c", name] | ["som", "c", name] => Some(name),
        _ => None,
    }
}

fn is_libc_source(source: &Source) -> bool {
    let dir = crate::stdlib::root().map(|root| root.join(toolchain::LIBC_BINDINGS_DIR));
    dir.is_some_and(|dir| FilePath::new(source.identifier()).starts_with(dir))
}

/// A branch of an `@if` on the system.
struct Branch {
    /// From its first statement to its last.
    within: Span,
    /// What it's compiled for.
    systems: Systems,
    /// Not for the target: never resolved.
    left_out: bool,
}

fn branches(ast: &Ast) -> Vec<Branch> {
    let mut out = Vec::new();
    for (id, stmt) in ast.stmts.iter() {
        let Stmt::If {
            condition,
            then,
            otherwise,
        } = stmt
        else {
            continue;
        };
        if !ast.settled.contains(&id) {
            continue;
        }
        let Some(systems) = systems_of(ast, *condition) else {
            continue;
        };
        let taken = ast.decided.get(&id).copied();
        if let Some(within) = covering(ast, then) {
            let left_out = taken == Some(false);
            out.push(Branch {
                within,
                systems,
                left_out,
            });
        }
        if let Some(within) = otherwise.as_deref().and_then(|o| covering(ast, o)) {
            let left_out = taken == Some(true);
            out.push(Branch {
                within,
                systems: systems.not(),
                left_out,
            });
        }
    }
    out
}

/// From the start of the first of `stmts` to the end of the last.
fn covering(ast: &Ast, stmts: &[Id<Stmt>]) -> Option<Span> {
    let first = ast.stmts.span(*stmts.first()?);
    let last = ast.stmts.span(*stmts.last()?);
    let mut span = first.clone();
    span.length = last.start_offset + last.length - first.start_offset;
    span.end = last.end;
    Some(span)
}

fn contains(within: &Span, span: &Span) -> bool {
    within.source.identifier() == span.source.identifier()
        && within.start_offset <= span.start_offset
        && span.start_offset + span.length <= within.start_offset + within.length
}

/// What a function uses that's not on every system: a C library name, or a function of
/// the program's.
#[derive(Debug, Clone)]
enum Need {
    Libc(String),
    /// One of WGSL's, from an `extern "wgsl"` block.
    Gl(String),
    Func(Id<Func>),
}

/// A call or name in a function, and the systems the code around it is compiled for.
struct Use {
    site: Span,
    need: Need,
    compiled_for: Systems,
}

/// The systems each function can run on, worked out from what it uses: a function can't run
/// where something it calls, outside an `@if` that leaves that system out, can't.
pub struct Inferred {
    systems: HashMap<Id<Func>, Systems>,
    uses: HashMap<Id<Func>, Vec<Use>>,
}

impl Inferred {
    fn of(&self, f: Id<Func>) -> Systems {
        self.systems.get(&f).copied().unwrap_or(Systems::ALL)
    }

    fn needs(&self, need: &Need) -> Systems {
        match need {
            Need::Libc(name) => libc_names().get(name).copied().unwrap_or(Systems::OSES),
            Need::Gl(_) => Systems::GPU,
            Need::Func(f) => self.of(*f),
        }
    }

    /// The first use in `f` that can't run on `system`, one system.
    fn blocker(&self, f: Id<Func>, system: Systems) -> Option<&Use> {
        (self.uses.get(&f)?.iter())
            .find(|u| u.compiled_for.has(system) && !self.needs(&u.need).has(system))
    }
}

pub fn infer(ast: &Ast, names: &Names) -> Inferred {
    let mut uses = HashMap::new();
    for (f, func) in ast.funcs.iter() {
        let Some(body) = func.body else { continue };
        let mut found = Vec::new();
        Walk {
            ast,
            names,
            found: &mut found,
        }
        .expr(body, Systems::ALL);
        if !found.is_empty() {
            uses.insert(f, found);
        }
    }
    let mut inferred = Inferred {
        systems: HashMap::new(),
        uses,
    };
    // Each pass can only take systems away, so it settles.
    loop {
        let mut changed = false;
        for (&f, found) in &inferred.uses {
            let lost = (found.iter()).fold(Systems::NONE, |lost, u| {
                lost.or(u.compiled_for.without(inferred.needs(&u.need)))
            });
            let systems = inferred.of(f).without(lost);
            if systems != inferred.of(f) {
                inferred.systems.insert(f, systems);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    inferred
}

/// Collects the uses in a function body, knowing the systems each is compiled for.
struct Walk<'a, 'u> {
    ast: &'a Ast,
    names: &'a Names,
    found: &'u mut Vec<Use>,
}

impl Walk<'_, '_> {
    fn expr(&mut self, id: Id<Expr>, compiled_for: Systems) {
        if let Some(need) = self.need(id) {
            self.found.push(Use {
                site: self.ast.exprs.span(id).clone(),
                need,
                compiled_for,
            });
        }
        self.ast.exprs[id].for_each_child(|child| self.child(child, compiled_for));
    }

    fn child(&mut self, child: Child, compiled_for: Systems) {
        match child {
            Child::Expr(e) => self.expr(e, compiled_for),
            Child::Stmt(s) => self.stmt(s, compiled_for),
        }
    }

    fn stmt(&mut self, id: Id<Stmt>, compiled_for: Systems) {
        // `@if GPU`: both branches are compiled, the first for the GPU and the other not.
        if let (
            Some(&then_gpu),
            Stmt::If {
                condition,
                then,
                otherwise,
            },
        ) = (self.ast.gpu_ifs.get(&id), &self.ast.stmts[id])
        {
            self.expr(*condition, compiled_for);
            let (on_then, on_otherwise) = match then_gpu {
                true => (Systems::GPU, Systems::OSES),
                false => (Systems::OSES, Systems::GPU),
            };
            for &s in then {
                self.stmt(s, compiled_for.and(on_then));
            }
            for &s in otherwise.as_deref().unwrap_or_default() {
                self.stmt(s, compiled_for.and(on_otherwise));
            }
            return;
        }
        let systems = match &self.ast.stmts[id] {
            Stmt::If { condition, .. } if self.ast.settled.contains(&id) => {
                systems_of(self.ast, *condition)
            }
            _ => None,
        };
        let (
            Some(systems),
            Stmt::If {
                condition,
                then,
                otherwise,
            },
        ) = (systems, &self.ast.stmts[id])
        else {
            return self.ast.stmts[id].for_each_child(|child| self.child(child, compiled_for));
        };
        self.expr(*condition, compiled_for);
        // A branch left out for this target isn't resolved: `check_left_out` has it. On the
        // GPU, the target's branch is taken.
        let taken = self.ast.decided.get(&id).copied();
        let gpu_if = |taken_here: bool| match taken_here {
            true => Systems::GPU,
            false => Systems::NONE,
        };
        if taken != Some(false) {
            let on = systems.and(Systems::OSES).or(gpu_if(taken == Some(true)));
            for &s in then {
                self.stmt(s, compiled_for.and(on));
            }
        }
        if taken != Some(true) {
            let on = (systems.not().and(Systems::OSES)).or(gpu_if(taken == Some(false)));
            for &s in otherwise.as_deref().unwrap_or_default() {
                self.stmt(s, compiled_for.and(on));
            }
        }
    }

    /// What expression `id` names that might not be on every system.
    fn need(&self, id: Id<Expr>) -> Option<Need> {
        if !matches!(self.ast.exprs[id], Expr::Var(_) | Expr::Call { .. }) {
            return None;
        }
        let def = self.names.def(self.names.expr(id)?);
        match def.kind {
            _ if is_libc_source(&def.span.source) => Some(Need::Libc(def.name.to_string())),
            DefKind::Func(f) if self.ast.gpu_builtins.contains(&f) => {
                Some(Need::Gl(def.name.to_string()))
            }
            DefKind::Func(f) => Some(Need::Func(f)),
            _ => None,
        }
    }
}

/// A function everything else is reached from, which has to run on every system: `main`,
/// a `@test`, or what the standard library offers.
fn is_entry(ast: &Ast, names: &Names, f: Id<Func>) -> bool {
    let func = &ast.funcs[f];
    let def = names.def(names.func(f));
    let main = *func.name.name == *som_lang::ENTRY_POINT && def.module == 0;
    let in_std = crate::stdlib::contains(FilePath::new(def.span.source.identifier()));
    let offered =
        in_std && func.visibility == Visibility::Public && !is_libc_source(&def.span.source);
    main || func.test || offered
}

/// An entry point that, through what it calls, uses what some system doesn't have.
#[derive(Debug)]
pub struct CantRun {
    function: String,
    missing: Systems,
    /// One for each call in the entry point that some system can't run.
    reasons: Vec<Reason>,
    in_std: bool,
}

#[derive(Debug)]
struct Reason {
    /// The call in the entry point.
    site: Span,
    /// What's called, from the entry point down to what a system doesn't have.
    chain: Vec<String>,
    /// The systems the last of them is on.
    on: Systems,
}

impl CantRun {
    pub fn span(&self) -> &Span {
        &self.reasons[0].site
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        // What each reason comes down to: `c::fork`, `c::setenv`.
        let mut unavailable: Vec<&str> = Vec::new();
        for reason in &self.reasons {
            let last = reason.chain.last().map_or("", String::as_str);
            if !unavailable.contains(&last) {
                unavailable.push(last);
            }
        }
        let verb = if unavailable.len() == 1 { " is" } else { " are" };
        let mut d = Diagnostic::error(msg![
            Message::list(unavailable.iter().map(m::func), " and "),
            verb,
            " not available on ",
            self.missing.names()
        ]);
        for (i, reason) in self.reasons.iter().enumerate() {
            let label = msg!["only on ", reason.on.names()];
            d = d.with_label(match i {
                0 => Label::primary(reason.site.clone(), label),
                _ => Label::secondary(reason.site.clone(), label),
            });
        }
        for reason in &self.reasons {
            let first = reason.chain.first().cloned().unwrap_or_default();
            if reason.chain.len() > 1 {
                let mut chain = vec![self.function.clone()];
                chain.extend(reason.chain.iter().cloned());
                d = d.with_hint(msg![m::code(chain.join(" → "))]);
            }
            d = d.with_hint(msg![
                "call ",
                m::func(&first),
                " inside ",
                m::code(format!("@if {} {{ … }}", reason.on.condition(self.in_std))),
            ]);
        }
        d.with_hint("with something else for the other systems")
    }
}

/// The entry points that can't run on the systems `checking` is for, each with why.
pub fn check_entries(
    ast: &Ast,
    names: &Names,
    inferred: &Inferred,
    checking: Checking,
) -> Vec<CantRun> {
    let systems = match checking {
        Checking::Target(os) => Systems::of(os),
        Checking::EverySystem => Systems::OSES,
    };
    let mut errors = Vec::new();
    for &f in inferred.systems.keys() {
        // A shader runs on the GPU; an entry point on the systems it's compiled for.
        let checked = match (ast.funcs[f].stage.is_some(), is_entry(ast, names, f)) {
            (true, false) => Systems::GPU,
            (true, true) => systems.or(Systems::GPU),
            (false, true) => systems,
            (false, false) => continue,
        };
        let missing = checked.without(inferred.of(f));
        if missing.is_empty() {
            continue;
        }
        let mut reasons: Vec<Reason> = Vec::new();
        for system in missing.each() {
            let Some(first) = inferred.blocker(f, system) else {
                continue;
            };
            if reasons.iter().any(|r| r.site == first.site) {
                continue;
            }
            let (chain, on) = chain(ast, inferred, &first.need, system);
            reasons.push(Reason {
                site: first.site.clone(),
                chain,
                on,
            });
        }
        if reasons.is_empty() {
            continue;
        }
        reasons.sort_by_key(|r| r.site.start_offset);
        let site = &ast.funcs[f].name.span;
        errors.push(CantRun {
            function: ast.funcs[f].name.name.to_string(),
            missing,
            reasons,
            in_std: crate::stdlib::contains(FilePath::new(site.source.identifier())),
        });
    }
    errors.sort_by_key(|e| {
        (
            e.span().source.identifier().to_string(),
            e.span().start_offset,
        )
    });
    errors
}

/// What `need` calls, down to what `os` doesn't have, and the systems that has.
fn chain(ast: &Ast, inferred: &Inferred, need: &Need, system: Systems) -> (Vec<String>, Systems) {
    let mut chain = Vec::new();
    let mut need = need.clone();
    let mut seen = HashSet::new();
    loop {
        match need {
            Need::Libc(name) => {
                chain.push(format!("c::{name}"));
                let on = libc_names().get(&name).copied().unwrap_or(Systems::NONE);
                return (chain, on);
            }
            Need::Gl(name) => {
                chain.push(format!("gl::{name}"));
                return (chain, Systems::GPU);
            }
            Need::Func(g) => {
                chain.push(ast.funcs[g].name.name.to_string());
                match inferred.blocker(g, system) {
                    Some(next) if seen.insert(g) => need = next.need.clone(),
                    _ => return (chain, inferred.of(g)),
                }
            }
        }
    }
}

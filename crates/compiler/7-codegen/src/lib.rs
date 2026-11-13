//! Code generation: MIR to C.
//!
//! The output is a single self-contained C11 file, written to read like the
//! program it came from:
//!
//! | MIR                         | C                                                      |
//! |-----------------------------|--------------------------------------------------------|
//! | struct `geo::Point`         | `struct geo_Point { int32_t x; int32_t y; }`           |
//! | function `Point::sum`       | `static int32_t Point_sum(struct Point* self)`         |
//! | local                       | a variable named after its som name; temporaries `_N`  |
//! | block `bbN`                 | a label, if anything jumps to it                       |
//! | place `(*self).0`           | `self->x`                                              |
//! | `StorageLive`/`StorageDead` | nothing                                                |
//!
//! A temporary that's used once, by the very next statement, is folded into
//! that use: `_2 = a + b; _0 = _2; return _0;` becomes `return a + b;`. C
//! leaves the order of evaluation within an expression open, and does
//! arithmetic on small integer types in `int`, so folding is limited to where
//! neither can change the meaning (see [`Use::allows`]).
//!
//! The C compiler lays out structs and handles calling conventions, so neither
//! appears here.

use som_mir::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    /// The program has no `main` function to start from.
    NoEntryPoint,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::NoEntryPoint => f.write_str("the program has no `main` function"),
        }
    }
}

/// Generates a C program from `mir`.
pub fn emit_c(mir: &Mir) -> Result<String, CodegenError> {
    let entry = mir
        .funcs
        .iter()
        .position(|f| f.is_entry)
        .ok_or(CodegenError::NoEntryPoint)?;
    let names = Names::new(mir);
    let mut c = Emitter {
        mir,
        names,
        out: String::new(),
    };
    c.prelude();
    c.structs();
    c.declarations();
    for (i, func) in mir.funcs.iter().enumerate() {
        if let Some(body) = &func.body {
            c.function(FuncId(i as u32), func, body);
        }
    }
    c.main(FuncId(entry as u32));
    Ok(c.out)
}

// --- Names ---

/// C's keywords, the names `<stdbool.h>` and `<stdint.h>` define, and `main`.
const C_RESERVED: &[&str] = &[
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
    "bool",
    "true",
    "false",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "intptr_t",
    "uintptr_t",
    "main",
];

/// Picks `base`, or `base_2`, `base_3`... for the first one that isn't taken or reserved,
/// and takes it. Names starting with `_` are reserved in C, so those get `prefix` in front.
fn unique(
    base: &str,
    prefix: &str,
    taken: &mut HashSet<String>,
    reserved: impl Fn(&str) -> bool,
) -> String {
    let base = if base.starts_with('_') {
        format!("{prefix}{base}")
    } else {
        base.to_string()
    };
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains(&name) || C_RESERVED.contains(&name.as_str()) || reserved(&name) {
        name = format!("{base}_{n}");
        n += 1;
    }
    taken.insert(name.clone());
    name
}

/// A som name as a C identifier: `Point::sum` → `Point_sum`, `id<&i32>` → `id_ref_i32`.
fn identifier(name: &str) -> String {
    let spelled = name
        .replace("::", "_")
        .replace("&mut ", "mutref_")
        .replace('&', "ref_")
        .replace('*', "ptr");
    let mut out = String::new();
    for c in spelled.chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        // One `_` at a time, so `id<i32>` is `id_i32`, not `id_i32_`.
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    let trimmed = out.trim_end_matches('_');
    if trimmed.is_empty() {
        "f".into()
    } else {
        trimmed.into()
    }
}

/// The C names of the program's structs, their fields and its functions.
struct Names {
    structs: HashMap<DefId, String>,
    fields: HashMap<DefId, Vec<String>>,
    funcs: Vec<String>,
}

impl Names {
    fn new(mir: &Mir) -> Names {
        // Struct tags have their own namespace in C.
        let mut tags = HashSet::new();
        let mut structs = HashMap::new();
        let mut fields = HashMap::new();
        for s in &mir.structs {
            structs.insert(
                s.def,
                unique(&identifier(&s.name), "s", &mut tags, |_| false),
            );
            let mut taken = HashSet::new();
            let names = s
                .fields
                .iter()
                .map(|(name, _)| unique(name, "f", &mut taken, |_| false))
                .collect();
            fields.insert(s.def, names);
        }

        // Externs keep their names, since that's what they link against; the rest avoid them.
        let externs: HashSet<String> = mir
            .funcs
            .iter()
            .filter_map(|f| f.link_name.clone())
            .collect();
        let mut taken = HashSet::new();
        let funcs = mir
            .funcs
            .iter()
            .map(|f| match &f.link_name {
                Some(link) => link.clone(),
                None if f.is_entry => unique("som_main", "f", &mut taken, |n| externs.contains(n)),
                None => unique(&identifier(&f.name), "f", &mut taken, |n| {
                    externs.contains(n)
                }),
            })
            .collect();

        Names {
            structs,
            fields,
            funcs,
        }
    }
}

/// C names for a function's locals: the som name where possible, so the C reads like the
/// source. The return value is `_0` and temporaries are `_N`. A name that would clash (a C
/// keyword, a function, a shadowed `let`, or one of the `_N` names) gets a `_2`, `_3`...
fn local_names(body: &Body, functions: &HashSet<String>) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut names: Vec<Option<String>> = body
        .locals
        .iter()
        .enumerate()
        .map(|(i, local)| {
            (i == 0 || local.name.is_none()).then(|| {
                taken.insert(format!("_{i}"));
                format!("_{i}")
            })
        })
        .collect();
    for (i, local) in body.locals.iter().enumerate() {
        if let (Some(source), None) = (&local.name, &names[i]) {
            names[i] = Some(unique(source, "v", &mut taken, |n| {
                functions.contains(n) || n.starts_with("som_")
            }));
        }
    }
    names
        .into_iter()
        .map(|n| n.expect("every local is named"))
        .collect()
}

// --- Folding temporaries ---

/// Where a local is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Use {
    /// The whole right-hand side of an assignment: `x = _3`.
    Assign,
    /// A call argument; C converts it to the parameter's type.
    Arg,
    /// A struct literal field; C converts it to the field's type.
    Field,
    /// An operand of an operator: `_3 + 1`, `!_3`.
    Operand,
    /// The condition of a branch.
    Branch,
    /// Anything else: borrowed, written through, or read through a projection.
    Other,
}

impl Use {
    /// Whether a temporary assigned `rvalue` can be folded into this use without changing
    /// what the program does.
    fn allows(self, rvalue: &Rvalue, narrow: bool) -> bool {
        match rvalue {
            // C doesn't fix the order of evaluation inside an expression, so a call is only
            // folded where nothing else is evaluated around it.
            Rvalue::Call { .. } => matches!(self, Use::Assign | Use::Branch),
            // C does arithmetic on 8- and 16-bit integers in `int`, so their results only fold
            // where C converts them back right away.
            Rvalue::Binary { op, .. } if narrow && is_arithmetic(*op) => {
                matches!(self, Use::Assign | Use::Arg | Use::Field)
            }
            _ => self != Use::Other,
        }
    }
}

fn is_arithmetic(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Subtract | BinOp::Multiply | BinOp::Divide | BinOp::Modulo
    )
}

/// How often each local is assigned and read in a body, and where it was read.
struct Usage {
    assigns: Vec<u32>,
    reads: Vec<u32>,
    read_as: Vec<Use>,
}

impl Usage {
    fn of(mir: &Mir, body: &Body) -> Usage {
        let n = body.locals.len();
        let mut u = Usage {
            assigns: vec![0; n],
            reads: vec![0; n],
            read_as: vec![Use::Other; n],
        };
        for block in &body.blocks {
            for stmt in &block.stmts {
                let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                    continue;
                };
                if dest.projection.is_empty() {
                    u.assigns[dest.local.0 as usize] += 1;
                } else {
                    u.read(dest.local, Use::Other);
                }
                match rvalue {
                    Rvalue::Use(op) => u.operand(op, Use::Assign),
                    Rvalue::Ref { place, .. } => u.read(place.local, Use::Other),
                    Rvalue::Not(op) => u.operand(op, Use::Operand),
                    Rvalue::Binary { lhs, rhs, .. } => {
                        u.operand(lhs, Use::Operand);
                        u.operand(rhs, Use::Operand);
                    }
                    Rvalue::Struct { fields, .. } => {
                        fields.iter().for_each(|f| u.operand(f, Use::Field))
                    }
                    Rvalue::Call { func, args } => {
                        // C converts an argument to its parameter's type, but a variadic
                        // function's extra arguments are only promoted, so they're operands.
                        let fixed = mir.func(*func).params.len();
                        for (i, a) in args.iter().enumerate() {
                            u.operand(a, if i < fixed { Use::Arg } else { Use::Operand });
                        }
                    }
                }
            }
            if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
                u.operand(condition, Use::Branch);
            }
        }
        u
    }

    fn operand(&mut self, op: &Operand, as_: Use) {
        if let Operand::Copy(place) | Operand::Move(place) = op {
            self.read(
                place.local,
                if place.projection.is_empty() {
                    as_
                } else {
                    Use::Other
                },
            );
        }
    }

    fn read(&mut self, local: Local, as_: Use) {
        self.reads[local.0 as usize] += 1;
        self.read_as[local.0 as usize] = as_;
    }
}

/// A C expression, and whether it can be used as an operand without parentheses.
#[derive(Clone)]
struct Expr {
    text: String,
    atomic: bool,
}

impl Expr {
    fn atom(text: String) -> Expr {
        Expr { text, atomic: true }
    }

    fn compound(text: String) -> Expr {
        Expr {
            text,
            atomic: false,
        }
    }

    /// The expression as an operand of an operator.
    fn operand(self) -> String {
        if self.atomic {
            self.text
        } else {
            format!("({})", self.text)
        }
    }
}

// --- Emitting ---

struct Emitter<'a> {
    mir: &'a Mir,
    names: Names,
    out: String,
}

/// The state while emitting one function.
struct FnState<'a> {
    body: &'a Body,
    locals: Vec<String>,
    usage: Usage,
    /// Temporaries computed but not yet written out, waiting to be folded into their use.
    pending: HashMap<Local, Expr>,
    /// How often the emitted code mentions each local, so only those mentioned get declared.
    mentioned: RefCell<HashMap<Local, u32>>,
}

impl<'a> Emitter<'a> {
    fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn prelude(&mut self) {
        self.line("// Generated by the som compiler.");
        self.line("#include <stdbool.h>");
        self.line("#include <stdint.h>");
        self.line("");
        self.line("typedef uint8_t som_unit;");
        self.line("");
    }

    fn struct_name(&self, def: DefId) -> String {
        format!("struct {}", self.names.structs[&def])
    }

    fn func_name(&self, id: FuncId) -> &str {
        &self.names.funcs[id.0 as usize]
    }

    fn ty(&self, ty: TypeId) -> String {
        match self.mir.types.get(ty) {
            Ty::Unit => "som_unit".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "uint32_t".into(),
            Ty::Str => "uint8_t".into(), // only ever behind a reference: `&str` is `uint8_t*`
            Ty::Int(int) => match int {
                IntTy::I8 => "int8_t",
                IntTy::I16 => "int16_t",
                IntTy::I32 => "int32_t",
                IntTy::I64 => "int64_t",
                IntTy::I128 => "__int128",
                IntTy::ISize => "intptr_t",
                IntTy::U8 => "uint8_t",
                IntTy::U16 => "uint16_t",
                IntTy::U32 => "uint32_t",
                IntTy::U64 => "uint64_t",
                IntTy::U128 => "unsigned __int128",
                IntTy::USize => "uintptr_t",
            }
            .into(),
            Ty::Float(FloatTy::F32) => "float".into(),
            Ty::Float(FloatTy::F64) => "double".into(),
            Ty::Pointer | Ty::Fn { .. } => "void*".into(),
            Ty::Ref { to, .. } => format!("{}*", self.ty(*to)),
            // A handle is a pointer to a struct only C sees inside.
            Ty::Struct(def) if self.mir.struct_def(*def).handle => {
                format!("{}*", self.struct_name(*def))
            }
            Ty::Struct(def) => self.struct_name(*def),
            Ty::Error | Ty::Param(_) | Ty::Var(_) => {
                unreachable!("codegen only sees checked, concrete types")
            }
        }
    }

    fn is_unit(&self, ty: TypeId) -> bool {
        matches!(self.mir.types.get(ty), Ty::Unit)
    }

    /// Whether `ty` is an integer that C does arithmetic on as `int`.
    fn is_narrow(&self, ty: TypeId) -> bool {
        matches!(
            self.mir.types.get(ty),
            Ty::Int(IntTy::I8 | IntTy::I16 | IntTy::U8 | IntTy::U16)
        )
    }

    /// The C return type of a function. Externs returning `()` return `void`.
    fn ret_ty(&self, func: &Function) -> String {
        if func.link_name.is_some() && self.is_unit(func.ret) {
            "void".into()
        } else {
            self.ty(func.ret)
        }
    }

    // --- Declarations ---

    /// Struct definitions, ordered so that a struct comes after any struct it contains by value.
    fn structs(&mut self) {
        let mir = self.mir;
        for s in &mir.structs {
            let name = self.struct_name(s.def);
            self.line(format!("{name};"));
        }
        let mut done = HashSet::new();
        for s in &mir.structs {
            self.define_struct(s.def, &mut done, &mut Vec::new());
        }
        self.line("");
    }

    fn define_struct(&mut self, def: DefId, done: &mut HashSet<DefId>, visiting: &mut Vec<DefId>) {
        // A struct that contains itself by value has no size; the type checker doesn't catch
        // that yet, so emit it anyway and let the C compiler report it.
        if done.contains(&def) || visiting.contains(&def) {
            return;
        }
        visiting.push(def);
        let s = self.mir.struct_def(def);
        for &(_, ty) in &s.fields {
            if let Ty::Struct(inner) = self.mir.types.get(ty) {
                self.define_struct(*inner, done, visiting);
            }
        }
        visiting.pop();
        done.insert(def);
        // Only C knows what's inside: the forward declaration is all there is.
        if s.handle {
            return;
        }

        let fields: Vec<String> = s
            .fields
            .iter()
            .zip(&self.names.fields[&def])
            .map(|(&(_, ty), name)| format!(" {} {name};", self.ty(ty)))
            .collect();
        // C doesn't allow empty structs.
        let fields = if fields.is_empty() {
            " som_unit unused;".to_string()
        } else {
            fields.concat()
        };
        let name = self.struct_name(def);
        self.line(format!("{name} {{{fields} }};"));
    }

    fn signature(&self, id: FuncId, func: &Function, params: Option<&[String]>) -> String {
        let params: Vec<String> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, &p)| match params {
                Some(names) => format!("{} {}", self.ty(p), names[i + 1]),
                None => self.ty(p),
            })
            .collect();
        let mut params = if params.is_empty() {
            "void".into()
        } else {
            params.join(", ")
        };
        if func.variadic {
            params.push_str(", ...");
        }
        let storage = if func.link_name.is_some() {
            ""
        } else {
            "static "
        };
        format!(
            "{storage}{} {}({params})",
            self.ret_ty(func),
            self.func_name(id)
        )
    }

    fn declarations(&mut self) {
        let mir = self.mir;
        let mut externs = HashSet::new();
        for (i, func) in mir.funcs.iter().enumerate() {
            // Two modules can declare the same C function; declare it once.
            if let Some(link) = &func.link_name
                && !externs.insert(link.clone())
            {
                continue;
            }
            let decl = self.signature(FuncId(i as u32), func, None);
            self.line(format!("{decl}; // {}", func.name));
        }
        self.line("");
    }

    fn main(&mut self, entry: FuncId) {
        let func = self.mir.func(entry);
        let call = format!("{}()", self.func_name(entry));
        self.line("int main(void) {");
        if self.is_unit(func.ret) {
            self.line(format!("    {call};"));
            self.line("    return 0;");
        } else {
            self.line(format!("    return (int){call};"));
        }
        self.line("}");
    }

    // --- Bodies ---

    fn function(&mut self, id: FuncId, func: &Function, body: &'a Body) {
        let functions: HashSet<String> = self.names.funcs.iter().cloned().collect();
        let mut f = FnState {
            body,
            locals: local_names(body, &functions),
            usage: Usage::of(self.mir, body),
            pending: HashMap::new(),
            mentioned: RefCell::new(HashMap::new()),
        };

        // Only blocks something jumps to need a label. Every block ends in a jump or a
        // return, so control never falls from one block into the next.
        let targets: HashSet<BlockId> = body
            .blocks
            .iter()
            .flat_map(|b| b.terminator.kind.successors())
            .collect();
        let mut code = Vec::new();
        for (i, block) in body.blocks.iter().enumerate() {
            if targets.contains(&BlockId(i as u32)) {
                code.push(format!("{}:", BlockId(i as u32)));
            }
            self.block(&mut f, block, &mut code);
        }

        let signature = self.signature(id, func, Some(&f.locals));
        self.line(format!("// {}", func.name));
        self.line(format!("{signature} {{"));
        let mentioned = f.mentioned.borrow();
        for (i, local) in body
            .locals
            .iter()
            .enumerate()
            .skip(body.arg_count + 1)
            .chain([(0, &body.locals[0])])
        {
            if mentioned.get(&Local(i as u32)).is_some_and(|&n| n > 0) {
                self.line(format!("    {} {};", self.ty(local.ty), f.locals[i]));
            }
        }
        for line in code {
            self.line(line);
        }
        self.line("}");
        self.line("");
    }

    fn block(&self, f: &mut FnState, block: &Block, code: &mut Vec<String>) {
        // `_0 = …` written as the block's last statement, which a `return` can take over.
        let mut returned: Option<(usize, String)> = None;

        for stmt in &block.stmts {
            let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                continue;
            };
            self.flush_unless_read(f, &reads(rvalue), code);

            let value = self.rvalue(f, rvalue);
            let local = dest.local;
            let is_temp = local != Local::RETURN && f.body.local(local).name.is_none();
            if is_temp && dest.projection.is_empty() {
                let narrow = self.is_narrow(f.body.local(local).ty);
                let (assigns, reads) = (
                    f.usage.assigns[local.0 as usize],
                    f.usage.reads[local.0 as usize],
                );
                if assigns == 1
                    && reads == 1
                    && f.usage.read_as[local.0 as usize].allows(rvalue, narrow)
                {
                    f.pending.insert(local, value);
                    continue;
                }
                // Nothing reads it: a call is kept for what it does, anything else dropped.
                if reads == 0 {
                    if matches!(rvalue, Rvalue::Call { .. }) {
                        code.push(format!("    {};", value.text));
                    }
                    returned = None;
                    continue;
                }
            }

            let place = self.place(f, dest);
            // A call to an extern returning `void` can't be assigned; call it, then store `()`.
            if let Rvalue::Call { func, .. } = rvalue
                && self.ret_ty(self.mir.func(*func)) == "void"
            {
                code.push(format!("    {}; {place} = 0;", value.text));
                returned = None;
                continue;
            }
            code.push(format!("    {place} = {};", value.text));
            returned = (local == Local::RETURN && dest.projection.is_empty())
                .then(|| (code.len() - 1, value.text));
        }

        let condition = match &block.terminator.kind {
            TerminatorKind::Branch { condition, .. } => vec![operand_local(condition)],
            _ => vec![],
        };
        self.flush_unless_read(
            f,
            &condition.into_iter().flatten().collect::<Vec<_>>(),
            code,
        );

        match &block.terminator.kind {
            TerminatorKind::Goto(target) => code.push(format!("    goto {target};")),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.operand(f, condition).text;
                code.push(format!(
                    "    if ({condition}) goto {then}; else goto {otherwise};"
                ));
            }
            TerminatorKind::Return => match returned {
                // `_0 = x; return _0;` is `return x;`, if nothing was written in between.
                Some((line, value)) if line == code.len() - 1 => {
                    code[line] = format!("    return {value};");
                    *f.mentioned.borrow_mut().entry(Local::RETURN).or_default() -= 1;
                }
                _ => {
                    *f.mentioned.borrow_mut().entry(Local::RETURN).or_default() += 1;
                    code.push(format!("    return {};", f.locals[0]));
                }
            },
            TerminatorKind::Unreachable => code.push("    __builtin_unreachable();".into()),
        }
    }

    /// Writes out every pending temporary that the next statement doesn't read, so that
    /// temporaries are only ever folded into the statement right after them.
    fn flush_unless_read(&self, f: &mut FnState, read: &[Local], code: &mut Vec<String>) {
        let mut flush: Vec<Local> = f
            .pending
            .keys()
            .filter(|l| !read.contains(l))
            .copied()
            .collect();
        flush.sort();
        for local in flush {
            let value = f.pending.remove(&local).expect("pending");
            *f.mentioned.borrow_mut().entry(local).or_default() += 1;
            code.push(format!(
                "    {} = {};",
                f.locals[local.0 as usize], value.text
            ));
        }
    }

    fn rvalue(&self, f: &mut FnState, rvalue: &Rvalue) -> Expr {
        match rvalue {
            Rvalue::Use(op) => self.operand(f, op),
            Rvalue::Ref { place, .. } => Expr::compound(format!("&{}", self.place(f, place))),
            Rvalue::Not(op) => Expr::compound(format!("!{}", self.operand(f, op).operand())),
            Rvalue::Binary { op, lhs, rhs } => {
                let pointer =
                    *op == BinOp::Add && self.operand_ty(f, lhs) == self.mir.types.pointer;
                let (l, r) = (
                    self.operand(f, lhs).operand(),
                    self.operand(f, rhs).operand(),
                );
                // `ptr + n` offsets by `n` bytes.
                if pointer {
                    return Expr::compound(format!("(void*)((uint8_t*){l} + {r})"));
                }
                Expr::compound(format!("{l} {} {r}", op.symbol()))
            }
            Rvalue::Struct { def, fields } => {
                let fields: Vec<String> =
                    fields.iter().map(|op| self.operand(f, op).text).collect();
                let fields = if fields.is_empty() {
                    "0".into()
                } else {
                    fields.join(", ")
                };
                Expr::atom(format!("({}){{{fields}}}", self.struct_name(*def)))
            }
            Rvalue::Call { func, args } => {
                let args: Vec<String> = args.iter().map(|a| self.operand(f, a).text).collect();
                Expr::atom(format!("{}({})", self.func_name(*func), args.join(", ")))
            }
        }
    }

    fn operand_ty(&self, f: &FnState, op: &Operand) -> TypeId {
        match op {
            Operand::Const(c) => c.ty,
            Operand::Copy(place) | Operand::Move(place) => self.place_ty(f, place),
        }
    }

    fn place_ty(&self, f: &FnState, place: &Place) -> TypeId {
        let mut ty = f.body.local(place.local).ty;
        for projection in &place.projection {
            ty = self.project(ty, *projection);
        }
        ty
    }

    fn project(&self, ty: TypeId, projection: Projection) -> TypeId {
        match (projection, self.mir.types.get(ty)) {
            (Projection::Deref, Ty::Ref { to, .. }) => *to,
            (Projection::Field(i), Ty::Struct(def)) => self.mir.struct_def(*def).fields[i].1,
            _ => unreachable!("validated MIR has well-typed places"),
        }
    }

    fn operand(&self, f: &mut FnState, op: &Operand) -> Expr {
        match op {
            // A temporary computed by the previous statement is folded in here.
            Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
                match f.pending.remove(&place.local) {
                    Some(value) => value,
                    None => Expr::atom(self.place(f, place)),
                }
            }
            // Moves are plain copies in C; the borrow checker already made sure they're sound.
            Operand::Copy(place) | Operand::Move(place) => Expr::atom(self.place(f, place)),
            Operand::Const(c) => Expr::atom(match &c.kind {
                // C gives a literal a type wide enough for its value, and converts it to
                // whatever it's combined with, so no cast is needed.
                ConstKind::Int(n) => n.to_string(),
                ConstKind::Float(x) => match self.mir.types.get(c.ty) {
                    Ty::Float(FloatTy::F32) => format!("{x:?}f"),
                    _ => format!("{x:?}"),
                },
                ConstKind::Bool(b) => (if *b { "true" } else { "false" }).into(),
                ConstKind::Str(s) => format!("(uint8_t*){}", c_string(s)),
                ConstKind::Unit => "0".into(),
                ConstKind::Func(id) => format!("(void*)&{}", self.func_name(*id)),
                // A compound literal zeroes anything: `(void*){0}`, `(Color){0}`.
                ConstKind::Zeroed => format!("({}){{0}}", self.ty(c.ty)),
            }),
        }
    }

    /// A place as a C lvalue: `x`, `p.x`, `self->x`, `(*r)`.
    fn place(&self, f: &FnState, place: &Place) -> String {
        *f.mentioned.borrow_mut().entry(place.local).or_default() += 1;
        let mut s = f.locals[place.local.0 as usize].clone();
        let mut ty = f.body.local(place.local).ty;
        let mut projections = place.projection.iter().peekable();
        while let Some(&projection) = projections.next() {
            if projection == Projection::Deref
                && let Some(&&Projection::Field(i)) = projections.peek()
            {
                // `(*p).x` is `p->x`.
                projections.next();
                ty = self.project(ty, Projection::Deref);
                s = format!("{s}->{}", self.field_name(ty, i));
                ty = self.project(ty, Projection::Field(i));
                continue;
            }
            s = match projection {
                Projection::Field(i) => format!("{s}.{}", self.field_name(ty, i)),
                Projection::Deref => format!("(*{s})"),
            };
            ty = self.project(ty, projection);
        }
        s
    }

    fn field_name(&self, struct_ty: TypeId, index: usize) -> &str {
        let Ty::Struct(def) = self.mir.types.get(struct_ty) else {
            unreachable!("fields belong to structs")
        };
        &self.names.fields[def][index]
    }
}

/// The local a whole-local operand reads, if it is one.
fn operand_local(op: &Operand) -> Option<Local> {
    match op {
        Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
            Some(place.local)
        }
        _ => None,
    }
}

/// The whole locals an rvalue reads, which pending temporaries can be folded into.
fn reads(rvalue: &Rvalue) -> Vec<Local> {
    match rvalue {
        Rvalue::Use(op) | Rvalue::Not(op) => operand_local(op).into_iter().collect(),
        Rvalue::Binary { lhs, rhs, .. } => {
            [lhs, rhs].into_iter().filter_map(operand_local).collect()
        }
        Rvalue::Struct { fields: ops, .. } | Rvalue::Call { args: ops, .. } => {
            ops.iter().filter_map(operand_local).collect()
        }
        Rvalue::Ref { .. } => Vec::new(),
    }
}

/// A C string literal with the same bytes as `s`.
fn c_string(s: &str) -> String {
    let mut out = String::from("\"");
    for byte in s.bytes() {
        match byte {
            b'"' | b'\\' => write!(out, "\\{}", byte as char).unwrap(),
            0x20..=0x7e => out.push(byte as char),
            // Octal escapes are at most three digits, so a following digit can't extend them.
            _ => write!(out, "\\{byte:03o}").unwrap(),
        }
    }
    out.push('"');
    out
}

//! Code generation: MIR to C.
//!
//! The output is a single self-contained C11 file, written to read like the
//! program it came from:
//!
//! | MIR                         | C                                                      |
//! |-----------------------------|--------------------------------------------------------|
//! | struct `geo::Point`         | `struct geo_Point { int32_t x; int32_t y; }`           |
//! | function `Point::sum`       | `static int32_t Point_sum(struct Point* self)`         |
//! | `&mut T` parameter          | `T* restrict`                                          |
//! | local                       | its som name, declared where first assigned            |
//! | temporary holding a call    | named after the function: `x = Vector_x(&v)`           |
//! | other temporary             | `_N`, if it isn't folded away                          |
//! | block `bbN`                 | a label, if anything jumps to it                       |
//! | place `(*self).0`           | `self->x`                                              |
//! | `StorageLive`/`StorageDead` | nothing                                                |
//!
//! A temporary that's used once is folded into that use: `_2 = a + b; _0 = _2;
//! return _0;` becomes `return a + b;`. It can wait past statements that don't
//! assign what it reads or write memory it reads; one holding a call can't
//! wait at all, so effects stay in order. C leaves the order of evaluation
//! within an expression open, and does arithmetic on small integer types in
//! `int`, so folding is limited to where neither can change the meaning (see
//! [`Use::allows`]). Operands are parenthesized only where C's precedence
//! would group them differently.
//!
//! The C compiler lays out structs and handles calling conventions, so neither
//! appears here.

use som_mir::*;
use std::cell::{Cell, RefCell};
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
    let (entry, _) = mir
        .funcs
        .iter_with_ids()
        .find(|(_, f)| f.is_entry)
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
    for (id, func) in mir.funcs.iter_with_ids() {
        if let Some(body) = &func.body {
            c.function(id, func, body);
        }
    }
    c.main(entry);
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
        .replace('?', "_opt")
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
    /// Each struct's name, one for each set of type arguments: `Pair<i32>` is `Pair_i32`.
    structs: HashMap<(DefId, Vec<TypeId>), String>,
    /// The structs made for optional, array and slice types the program uses: whether an
    /// optional holds a value and the value; an array's elements; where a slice's elements
    /// start, and how many there are.
    generated: HashMap<TypeId, String>,
    fields: HashMap<DefId, Vec<String>>,
    funcs: HashMap<FuncId, String>,
}

impl Names {
    fn new(mir: &Mir) -> Names {
        // Struct tags have their own namespace in C.
        let mut tags = HashSet::new();
        let mut structs = HashMap::new();
        let mut fields = HashMap::new();
        for s in &mir.structs {
            structs.insert(
                (s.def, s.args.clone()),
                unique(&identifier(&s.name), "s", &mut tags, |_| false),
            );
            let names = match &s.variants {
                None => {
                    let mut taken = HashSet::new();
                    (s.fields.iter())
                        .map(|(name, _)| unique(name, "f", &mut taken, |_| false))
                        .collect()
                }
                // `tag`, then each variant's fields, reached through the union of variants:
                // `as.Rect.w`, `as.Circle._0`.
                Some(variants) => {
                    // The common fields sit beside the tag and the union.
                    let mut taken_common: HashSet<String> =
                        ["tag", "as"].into_iter().map(String::from).collect();
                    let mut names: Vec<String> = (s.fields.iter().enumerate())
                        .map(|(i, (name, _))| match i {
                            0 => "tag".to_string(),
                            _ if !name.contains('.') => {
                                unique(name, "f", &mut taken_common, |_| false)
                            }
                            _ => String::new(),
                        })
                        .collect();
                    let mut taken_variants = HashSet::new();
                    for (i, v) in variants.iter().enumerate() {
                        let variant = unique(&v.name, "v", &mut taken_variants, |_| false);
                        let mut taken = HashSet::new();
                        for &index in s.variant_fields(i) {
                            let name = &s.fields[index].0;
                            let Some((_, field)) = name.split_once('.') else {
                                continue;
                            };
                            // A position, `_0`, can't clash with a name.
                            let field = match field.parse::<usize>() {
                                Ok(n) => format!("_{n}"),
                                Err(_) => unique(field, "f", &mut taken, |_| false),
                            };
                            names[index] = format!("as.{variant}.{field}");
                        }
                    }
                    names
                }
            };
            fields.insert(s.def, names);
        }
        let mut generated = HashMap::new();
        for ty in used_generated(mir) {
            let name = match mir.types.get(ty) {
                Ty::Optional(inner) => format!("som_opt_{}", identifier(&mir.display_ty(*inner))),
                Ty::Array { element, len } => format!(
                    "som_arr_{}_{}",
                    identifier(&mir.display_ty(*element)),
                    mir.types.len(*len)
                ),
                Ty::Slice(element) => {
                    format!("som_slice_{}", identifier(&mir.display_ty(*element)))
                }
                Ty::Str => "som_str".to_string(),
                Ty::Range(int) => format!("som_range_{}", identifier(&mir.display_ty(*int))),
                _ => unreachable!("used_generated returns optionals, arrays, slices and text"),
            };
            generated.insert(ty, unique(&name, "s", &mut tags, |_| false));
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
            .iter_with_ids()
            .map(|(id, f)| match &f.link_name {
                Some(link) => (id, link.clone()),
                None if f.is_entry => (
                    id,
                    unique("som_main", "f", &mut taken, |n| externs.contains(n)),
                ),
                None => (
                    id,
                    unique(&identifier(&f.name), "f", &mut taken, |n| {
                        externs.contains(n)
                    }),
                ),
            })
            .collect();

        Names {
            structs,
            generated,
            fields,
            funcs,
        }
    }
}

/// The optional, array and slice types in the program's functions and structs.
fn used_generated(mir: &Mir) -> Vec<TypeId> {
    let mut todo: Vec<TypeId> = Vec::new();
    for func in &mir.funcs {
        todo.extend(&func.params);
        todo.push(func.ret);
        if let Some(body) = &func.body {
            todo.extend(body.locals.iter().map(|l| l.ty));
        }
    }
    for s in &mir.structs {
        todo.extend(s.fields.iter().map(|&(_, ty)| ty));
    }
    let mut seen = HashSet::new();
    let mut found = Vec::new();
    while let Some(ty) = todo.pop() {
        if !seen.insert(ty) {
            continue;
        }
        let t = mir.types.get(ty);
        if matches!(
            t,
            Ty::Optional(_) | Ty::Array { .. } | Ty::Slice(_) | Ty::Str | Ty::Range(_)
        ) {
            found.push(ty);
        }
        todo.extend(t.children());
    }
    // Sorted, so the output doesn't depend on hash order.
    found.sort();
    found
}

/// C names for a function's locals: the som name where possible, so the C reads like the
/// source. A temporary holding what a call returns is named after the function, `Vector::x`
/// as `x`; the return value is `_0` and other temporaries are `_N`. A name that would clash
/// (a C keyword, a function, a shadowed `let`, or one of the `_N` names) gets a `_2`, `_3`...
/// Indexed by local id; removed locals get an empty name.
fn local_names(mir: &Mir, body: &Body, functions: &HashSet<String>) -> Vec<String> {
    let mut callees: HashMap<Local, &str> = HashMap::new();
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        if let StatementKind::Assign(dest, Rvalue::Call { func, .. }) = &stmt.kind
            && dest.projection.is_empty()
        {
            callees.entry(dest.local).or_insert(&mir.func(*func).name);
        }
    }
    let temp = |id: Local, local: &LocalDecl| id != RETURN && local.name.is_none();
    let mut taken: HashSet<String> = HashSet::new();
    let mut names = vec![String::new(); body.locals.slots()];
    for (id, local) in body.locals.iter_with_ids() {
        if id == RETURN || temp(id, local) && !callees.contains_key(&id) {
            names[id.id] = format!("_{}", id.id);
            taken.insert(names[id.id].clone());
        }
    }
    let reserved = |n: &str| functions.contains(n) || n.starts_with("som_");
    for (id, local) in body.locals.iter_with_ids() {
        if let Some(source) = &local.name
            && names[id.id].is_empty()
        {
            names[id.id] = unique(source, "v", &mut taken, reserved);
        }
    }
    for (id, local) in body.locals.iter_with_ids() {
        if temp(id, local)
            && let Some(callee) = callees.get(&id)
        {
            // `som::data::Vector::x<f64>` is `x`.
            let callee = callee.split('<').next().unwrap_or(callee);
            let callee = callee.rsplit("::").next().unwrap_or(callee);
            names[id.id] = unique(&identifier(callee), "v", &mut taken, reserved);
        }
    }
    names
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
    /// An array index: `items[_3]`.
    Index,
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
            Rvalue::Intrinsic { intrinsic, .. } if intrinsic.has_effect() => {
                matches!(self, Use::Assign | Use::Branch)
            }
            // A loop, not an expression.
            Rvalue::Repeat { operand, .. } if !is_zero(operand) => false,
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
        let n = body.locals.slots();
        let mut u = Usage {
            assigns: vec![0; n],
            reads: vec![0; n],
            read_as: vec![Use::Other; n],
        };
        for block in &body.blocks {
            for stmt in &block.stmts {
                // A drop reads what it drops, and its flag.
                if let StatementKind::Drop { place, flag, .. } = &stmt.kind {
                    u.read(place.local, Use::Other);
                    place
                        .index_locals()
                        .for_each(|index| u.read(index, Use::Other));
                    if let Some(flag) = flag {
                        u.read(*flag, Use::Other);
                    }
                    continue;
                }
                let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                    continue;
                };
                if dest.projection.is_empty() {
                    u.assigns[dest.local.id] += 1;
                } else {
                    u.read(dest.local, Use::Other);
                }
                dest.index_locals()
                    .for_each(|index| u.read(index, Use::Index));
                match rvalue {
                    Rvalue::Use(op) => u.operand(op, Use::Assign),
                    Rvalue::Ref { place, .. } => {
                        u.read(place.local, Use::Other);
                        place
                            .index_locals()
                            .for_each(|index| u.read(index, Use::Index));
                    }
                    Rvalue::Not(op) => u.operand(op, Use::Operand),
                    // A field of the compound literal it becomes.
                    Rvalue::Some(op) => u.operand(op, Use::Field),
                    Rvalue::Array { items, .. } => {
                        items.iter().for_each(|item| u.operand(item, Use::Field))
                    }
                    // Read once per element, by a loop.
                    Rvalue::Repeat { operand, .. } => u.operand(operand, Use::Other),
                    Rvalue::Cast { operand, .. } => u.operand(operand, Use::Operand),
                    // Fields of the compound literal they become.
                    Rvalue::Range { start, end, .. } => {
                        u.operand(start, Use::Field);
                        u.operand(end, Use::Field);
                    }
                    Rvalue::MakeSlice { start, len, .. } => {
                        u.operand(start, Use::Field);
                        u.operand(len, Use::Field);
                    }
                    Rvalue::Binary { lhs, rhs, .. } => {
                        u.operand(lhs, Use::Operand);
                        u.operand(rhs, Use::Operand);
                    }
                    Rvalue::Struct { fields, .. } => {
                        fields.iter().for_each(|f| u.operand(f, Use::Field))
                    }
                    Rvalue::Intrinsic { args, .. } => {
                        args.iter().for_each(|a| u.operand(a, Use::Operand))
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
            place
                .index_locals()
                .for_each(|index| self.read(index, Use::Index));
        }
    }

    fn read(&mut self, local: Local, as_: Use) {
        self.reads[local.id] += 1;
        self.read_as[local.id] = as_;
    }
}

/// How tightly a C expression binds, loosest first. `Other` is anything that's always
/// parenthesized as an operand: casts, `!`, `&`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Prec {
    Other,
    Or,
    And,
    Equality,
    Relational,
    Additive,
    Multiplicative,
    Atom,
}

impl Prec {
    fn of(op: BinOp) -> Prec {
        match op {
            BinOp::Or => Prec::Or,
            BinOp::And => Prec::And,
            BinOp::Equals | BinOp::NotEquals => Prec::Equality,
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => Prec::Relational,
            BinOp::Add | BinOp::Subtract => Prec::Additive,
            BinOp::Multiply | BinOp::Divide | BinOp::Modulo => Prec::Multiplicative,
        }
    }

    fn is_arithmetic(self) -> bool {
        matches!(self, Prec::Additive | Prec::Multiplicative)
    }

    fn is_logical(self) -> bool {
        matches!(self, Prec::And | Prec::Or)
    }
}

/// A C expression, and how tightly it binds.
#[derive(Clone)]
struct Expr {
    text: String,
    prec: Prec,
}

impl Expr {
    fn atom(text: String) -> Expr {
        Expr {
            text,
            prec: Prec::Atom,
        }
    }

    fn compound(text: String) -> Expr {
        Expr {
            text,
            prec: Prec::Other,
        }
    }

    /// The expression as an operand of a unary operator or cast.
    fn operand(self) -> String {
        if self.prec == Prec::Atom {
            self.text
        } else {
            format!("({})", self.text)
        }
    }

    /// The expression as the left or right operand of `op`. Parentheses are left out only
    /// where C's precedence gives the same grouping and compilers don't warn about mixing:
    /// `a * b + c`, `a - b + c`, `a + b < c`, `a < b && c`.
    fn operand_of(self, op: BinOp, left: bool) -> String {
        let parent = Prec::of(op);
        let bare = match self.prec {
            Prec::Atom => true,
            child if child == parent => left && (child.is_arithmetic() || child.is_logical()),
            child if child.is_arithmetic() => child > parent && !parent.is_logical(),
            Prec::Equality | Prec::Relational => parent.is_logical(),
            _ => false,
        };
        if bare {
            self.text
        } else {
            format!("({})", self.text)
        }
    }
}

/// A temporary computed but not yet written out, waiting to be folded into where it's read.
struct Pending {
    expr: Expr,
    /// The locals its value depends on: if one is assigned, it has to be written out first.
    reads: HashSet<Local>,
    /// Whether it reads memory something else could change: through a reference, or a local
    /// whose address is taken.
    memory: bool,
    /// Whether it has an effect, a call: then it can't move past anything.
    effect: bool,
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
    pending: RefCell<HashMap<Local, Pending>>,
    /// What the temporaries folded into the expression being built depend on: their reads,
    /// whether they read memory, and whether they have an effect.
    folded: RefCell<(HashSet<Local>, bool, bool)>,
    /// Locals whose address is taken, so a write through a reference can change them.
    address_taken: HashSet<Local>,
    /// How often the emitted code mentions each local, so only those mentioned get declared.
    mentioned: RefCell<HashMap<Local, u32>>,
    /// The line of code each local is first mentioned on, where it can be declared.
    first: RefCell<HashMap<Local, usize>>,
    /// The line of code being written.
    line: Cell<usize>,
}

impl FnState<'_> {
    fn mention(&self, local: Local) {
        *self.mentioned.borrow_mut().entry(local).or_default() += 1;
        self.first
            .borrow_mut()
            .entry(local)
            .or_insert(self.line.get());
    }

    /// Takes the pending temporary `local`, to fold it into the expression being built.
    fn fold(&self, local: Local) -> Option<Expr> {
        let p = self.pending.borrow_mut().remove(&local)?;
        let mut folded = self.folded.borrow_mut();
        folded.0.extend(p.reads);
        folded.1 |= p.memory;
        folded.2 |= p.effect;
        Some(p.expr)
    }
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
    }

    fn struct_name(&self, def: DefId, args: &[TypeId]) -> String {
        format!("struct {}", self.names.structs[&(def, args.to_vec())])
    }

    fn func_name(&self, id: FuncId) -> &str {
        &self.names.funcs[&id]
    }

    fn ty(&self, ty: TypeId) -> String {
        match self.mir.types.get(ty) {
            Ty::Unit => "som_unit".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "uint32_t".into(),
            Ty::Str => format!("struct {}", self.names.generated[&ty]),
            Ty::CStr => "uint8_t*".into(),
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
            Ty::Ptr(to) => format!("{}*", self.ty(*to)),
            // A slice is both: where its elements start, and how many there are.
            Ty::Ref { to, .. } if matches!(self.mir.types.get(*to), Ty::Slice(_) | Ty::Str) => {
                self.ty(*to)
            }
            Ty::Ref { to, .. } => format!("{}*", self.ty(*to)),
            // A handle is a pointer to a struct only C sees inside.
            Ty::Struct(def, args) if self.mir.struct_def(*def, args).handle => {
                format!("{}*", self.struct_name(*def, args))
            }
            Ty::Struct(def, args) => self.struct_name(*def, args),
            Ty::Optional(_) | Ty::Array { .. } | Ty::Slice(_) | Ty::Range(_) => {
                format!("struct {}", self.names.generated[&ty])
            }
            Ty::Error
            | Ty::Param(_)
            | Ty::Var(_)
            | Ty::Value(_)
            | Ty::Add(..)
            | Ty::Mul(..)
            | Ty::Assoc { .. } => {
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

    /// Struct, optional and array definitions, ordered so that each comes after any it
    /// contains by value.
    fn structs(&mut self) {
        let mir = self.mir;
        let mut types: Vec<TypeId> = mir
            .structs
            .iter()
            .filter_map(|s| mir.types.lookup(&Ty::Struct(s.def, s.args.clone())))
            .collect();
        let mut generated: Vec<TypeId> = self.names.generated.keys().copied().collect();
        generated.sort();
        types.extend(generated);
        for &ty in &types {
            let name = self.ty(ty);
            // A handle's type is a pointer to the struct.
            self.line(format!("{};", name.trim_end_matches('*')));
        }
        let mut done = HashSet::new();
        for &ty in &types {
            self.define(ty, &mut done, &mut Vec::new());
        }
        self.line("");
    }

    /// Defines the struct, optional or array `ty`, after what it contains by value.
    fn define(&mut self, ty: TypeId, done: &mut HashSet<TypeId>, visiting: &mut Vec<TypeId>) {
        // A struct that contains itself by value has no size; the type checker doesn't catch
        // that yet, so emit it anyway and let the C compiler report it.
        if done.contains(&ty) || visiting.contains(&ty) {
            return;
        }
        let contains: Vec<TypeId> = match self.mir.types.get(ty) {
            Ty::Struct(def, args) => (self.mir.struct_def(*def, args).fields.iter())
                .map(|f| f.1)
                .collect(),
            Ty::Optional(inner) | Ty::Array { element: inner, .. } | Ty::Range(inner) => {
                vec![*inner]
            }
            // Its elements are behind a pointer, so they don't have to come first.
            Ty::Slice(_) | Ty::Str => Vec::new(),
            _ => return,
        };
        visiting.push(ty);
        for inner in contains {
            // A slice is a struct of its own, held by value even though it's a reference.
            let inner = match self.mir.types.get(inner) {
                Ty::Ref { to, .. } if matches!(self.mir.types.get(*to), Ty::Slice(_) | Ty::Str) => {
                    *to
                }
                _ => inner,
            };
            self.define(inner, done, visiting);
        }
        visiting.pop();
        done.insert(ty);
        match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => self.define_struct(def, &args),
            Ty::Optional(inner) => {
                let value = self.ty(inner);
                self.line(format!("{} {{ bool some; {value} value; }};", self.ty(ty)));
            }
            Ty::Slice(element) => {
                let element = self.ty(element);
                self.line(format!(
                    "{} {{ {element}* ptr; uintptr_t len; }};",
                    self.ty(ty)
                ));
            }
            Ty::Str => {
                self.line(format!(
                    "{} {{ uint8_t* ptr; uintptr_t len; }};",
                    self.ty(ty)
                ));
            }
            Ty::Range(int) => {
                let int = self.ty(int);
                self.line(format!("{} {{ {int} start; {int} end; }};", self.ty(ty)));
            }
            // C has no empty arrays; one that's never read stands in.
            Ty::Array { element, len } => {
                let element = self.ty(element);
                let len = self.mir.types.len(len).max(1);
                self.line(format!("{} {{ {element} items[{len}]; }};", self.ty(ty)));
            }
            _ => {}
        }
    }

    fn define_struct(&mut self, def: DefId, args: &[TypeId]) {
        let s = self.mir.struct_def(def, args);
        // Only C knows what's inside: the forward declaration is all there is.
        if s.handle {
            return;
        }

        let names = &self.names.fields[&def];
        if let Some(variants) = &s.variants {
            // `struct Shape { uint32_t tag; float base; union { struct { float _0; } Circle; … }
            // as; };`
            let common: String = (s.fields.iter().zip(names).skip(1))
                .filter(|(_, name)| !name.starts_with("as."))
                .map(|(&(_, ty), name)| format!(" {} {name};", self.ty(ty)))
                .collect();
            let mut members = Vec::new();
            for (i, _) in variants.iter().enumerate() {
                let own: Vec<usize> = (s.variant_fields(i).iter().copied())
                    .filter(|&index| names[index].starts_with("as."))
                    .collect();
                if own.is_empty() {
                    continue;
                }
                let mut variant = String::new();
                let mut fields = String::new();
                for index in own {
                    let mut parts = names[index].split('.').skip(1);
                    variant = parts.next().unwrap_or_default().to_string();
                    let field = parts.next().unwrap_or_default();
                    fields.push_str(&format!(" {} {field};", self.ty(s.fields[index].1)));
                }
                members.push(format!(" struct {{{fields} }} {variant};"));
            }
            let union = if members.is_empty() {
                String::new()
            } else {
                format!(" union {{{} }} as;", members.concat())
            };
            let name = self.struct_name(def, args);
            self.line(format!("{name} {{ uint32_t tag;{common}{union} }};"));
            return;
        }
        let fields: Vec<String> = s
            .fields
            .iter()
            .zip(names)
            .map(|(&(_, ty), name)| format!(" {} {name};", self.ty(ty)))
            .collect();
        // C doesn't allow empty structs.
        let fields = if fields.is_empty() {
            " som_unit unused;".to_string()
        } else {
            fields.concat()
        };
        let name = self.struct_name(def, args);
        self.line(format!("{name} {{{fields} }};"));
    }

    fn signature(&self, id: FuncId, func: &Function, params: Option<&[String]>) -> String {
        let params: Vec<String> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let mut ty = self.ty(p);
                // The borrow checker makes sure nothing else reaches what a `&mut`
                // parameter points to while the call runs, which is what `restrict`
                // promises C. Extern functions keep the signature C declared.
                if func.link_name.is_none()
                    && let Ty::Ref { mutable: true, to } = self.mir.types.get(p)
                    && !matches!(self.mir.types.get(*to), Ty::Slice(_))
                {
                    ty.push_str(" restrict");
                }
                match params {
                    Some(names) => format!("{ty} {}", names[i + 1]),
                    None => ty,
                }
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

    /// The C functions the program calls, then its own functions, each with its som name.
    fn declarations(&mut self) {
        let mir = self.mir;
        let mut externs = HashSet::new();
        let mut any = false;
        for (id, func) in mir.funcs.iter_with_ids() {
            // Two modules can declare the same C function; declare it once.
            if let Some(link) = &func.link_name
                && externs.insert(link.clone())
            {
                let decl = self.signature(id, func, None);
                self.line(format!("{decl};"));
                any = true;
            }
        }
        if any {
            self.line("");
        }
        for (id, func) in mir.funcs.iter_with_ids() {
            if func.link_name.is_none() {
                let decl = self.signature(id, func, None);
                self.line(format!("{decl}; // {}", func.name));
            }
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
        let functions: HashSet<String> = self.names.funcs.values().cloned().collect();
        let mut f = FnState {
            body,
            locals: local_names(self.mir, body, &functions),
            usage: Usage::of(self.mir, body),
            pending: RefCell::new(HashMap::new()),
            folded: RefCell::default(),
            address_taken: address_taken(body),
            mentioned: RefCell::new(HashMap::new()),
            first: RefCell::default(),
            line: Cell::new(0),
        };

        // Only blocks something jumps to need a label. Every block ends in a jump or a
        // return, so control never falls from one block into the next.
        let targets: HashSet<BlockId> = body
            .blocks
            .iter()
            .flat_map(|b| b.terminator.kind.successors())
            .collect();
        let mut code = Vec::new();
        for (id, block) in body.blocks.iter_with_ids() {
            if targets.contains(&id) {
                code.push(format!("{id}:"));
            }
            self.block(&mut f, block, &mut code);
        }

        let signature = self.signature(id, func, Some(&f.locals));
        self.line(format!("// {}", func.name));
        self.line(format!("{signature} {{"));
        // A local is declared where it's first assigned, if that's the first time it's
        // mentioned: everything after is in its scope, as a function is one flat scope, and C
        // allows jumping past a declaration. Before C23 a label can't come right before one,
        // and anything else is declared at the top.
        let mentioned = f.mentioned.borrow();
        let first = f.first.borrow();
        for (id, local) in body
            .locals
            .iter_with_ids()
            .skip(body.arg_count + 1)
            .chain([(RETURN, &body.locals[RETURN])])
        {
            if !mentioned.get(&id).is_some_and(|&n| n > 0) {
                continue;
            }
            let (ty, name) = (self.ty(local.ty), &f.locals[id.id]);
            let assign = format!("    {name} = ");
            if let Some(&line) = first.get(&id)
                && code.get(line).is_some_and(|l| l.starts_with(&assign))
                && (line == 0 || !code[line - 1].ends_with(':'))
            {
                code[line] = format!("    {ty} {}", &code[line][4..]);
            } else {
                self.line(format!("    {ty} {name};"));
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
            // `som_drop_T(&x);`, or with a flag, only if it's set.
            if let StatementKind::Drop { place, glue, flag } = &stmt.kind {
                self.flush_except(f, &[], code);
                f.line.set(code.len());
                let call = format!("{}(&{})", self.func_name(*glue), self.place(f, place));
                code.push(match flag {
                    Some(flag) => {
                        f.mention(*flag);
                        format!("    if ({}) {call};", f.locals[flag.id])
                    }
                    None => format!("    {call};"),
                });
                returned = None;
                continue;
            }
            let StatementKind::Assign(dest, rvalue) = &stmt.kind else {
                continue;
            };
            self.flush_before(f, dest, rvalue, code);
            *f.folded.borrow_mut() = Default::default();
            f.line.set(code.len());

            // `[value; N]`: C has no expression for that, so a loop fills it in.
            if let Rvalue::Repeat { ty, operand } = rvalue
                && !is_zero(operand)
            {
                let Ty::Array { len, .. } = *self.mir.types.get(*ty) else {
                    unreachable!("repeats make arrays")
                };
                let len = self.mir.types.len(len);
                let value = self.operand(f, operand).text;
                let place = self.place(f, dest);
                code.push(format!(
                    "    for (uintptr_t som_i = 0; som_i < {len}; som_i++) {place}.items[som_i] = {value};"
                ));
                returned = None;
                continue;
            }

            let value = self.rvalue(f, rvalue);
            let local = dest.local;
            let is_temp = local != RETURN && f.body.local(local).name.is_none();
            if is_temp && dest.projection.is_empty() {
                let narrow = self.is_narrow(f.body.local(local).ty);
                let (assigns, reads) = (f.usage.assigns[local.id], f.usage.reads[local.id]);
                // A call to an extern returning `void` has no value to put where it's read.
                let void = matches!(rvalue, Rvalue::Call { func, .. }
                    if self.ret_ty(self.mir.func(*func)) == "void");
                if assigns == 1
                    && reads == 1
                    && !void
                    && f.usage.read_as[local.id].allows(rvalue, narrow)
                {
                    let pending = self.pending(f, rvalue, value);
                    f.pending.borrow_mut().insert(local, pending);
                    continue;
                }
                // Nothing reads it: a call is kept for what it does, anything else dropped.
                if reads == 0 {
                    let effect = match rvalue {
                        Rvalue::Call { .. } => true,
                        Rvalue::Intrinsic { intrinsic, .. } => intrinsic.has_effect(),
                        _ => false,
                    };
                    if effect {
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
            returned = (local == RETURN && dest.projection.is_empty())
                .then(|| (code.len() - 1, value.text));
        }

        let condition = match &block.terminator.kind {
            TerminatorKind::Branch { condition, .. } => vec![operand_local(condition)],
            _ => vec![],
        };
        self.flush_except(
            f,
            &condition.into_iter().flatten().collect::<Vec<_>>(),
            code,
        );

        f.line.set(code.len());
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
                    *f.mentioned.borrow_mut().entry(RETURN).or_default() -= 1;
                }
                _ => {
                    f.line.set(code.len());
                    f.mention(RETURN);
                    code.push(format!("    return {};", f.locals[0]));
                }
            },
            TerminatorKind::Unreachable => code.push("    __builtin_unreachable();".into()),
        }
    }

    /// The temporary assigned `rvalue`, written as `expr`, waiting to be folded: what it
    /// depends on, including the temporaries already folded into it.
    fn pending(&self, f: &FnState, rvalue: &Rvalue, expr: Expr) -> Pending {
        let (mut reads, mut memory, mut effect) = f.folded.take();
        for place in places(rvalue) {
            reads.extend(place.locals());
            memory |= place.projection.contains(&Projection::Deref);
        }
        memory |= reads.iter().any(|l| f.address_taken.contains(l));
        match rvalue {
            Rvalue::Call { .. } => effect = true,
            Rvalue::Intrinsic { intrinsic, .. } => {
                memory = true;
                effect |= intrinsic.has_effect();
            }
            _ => {}
        }
        Pending {
            expr,
            reads,
            memory,
            effect,
        }
    }

    /// Before `dest = rvalue`, writes out the pending temporaries it doesn't read and would
    /// change the value of: those that read a local it assigns, or memory it may write. A
    /// temporary with an effect is written out unless read right away, so effects stay in
    /// order.
    fn flush_before(&self, f: &FnState, dest: &Place, rvalue: &Rvalue, code: &mut Vec<String>) {
        let read = statement_reads(dest, rvalue);
        let pending = f.pending.borrow();
        let effect = matches!(rvalue, Rvalue::Call { .. })
            || matches!(rvalue, Rvalue::Intrinsic { intrinsic, .. } if intrinsic.has_effect())
            || read
                .iter()
                .any(|l| pending.get(l).is_some_and(|p| p.effect));
        let writes_memory = effect || !dest.projection.is_empty();
        let keep: Vec<Local> = (pending.iter())
            .filter(|(local, p)| {
                read.contains(local)
                    || !(p.effect || p.reads.contains(&dest.local) || p.memory && writes_memory)
            })
            .map(|(&local, _)| local)
            .collect();
        drop(pending);
        self.flush_except(f, &keep, code);
    }

    /// Writes out every pending temporary but those in `keep`.
    fn flush_except(&self, f: &FnState, keep: &[Local], code: &mut Vec<String>) {
        let mut flush: Vec<Local> = (f.pending.borrow().keys())
            .filter(|l| !keep.contains(l))
            .copied()
            .collect();
        flush.sort();
        for local in flush {
            let value = f.pending.borrow_mut().remove(&local).expect("pending");
            f.line.set(code.len());
            f.mention(local);
            code.push(format!("    {} = {};", f.locals[local.id], value.expr.text));
        }
    }

    fn rvalue(&self, f: &mut FnState, rvalue: &Rvalue) -> Expr {
        match rvalue {
            Rvalue::Use(op) => self.operand(f, op),
            // `&*p` is `p`: for a slice, that's the only way to write it.
            Rvalue::Ref { place, .. } if place.projection.last() == Some(&Projection::Deref) => {
                let mut inner = place.clone();
                inner.projection.pop();
                Expr::atom(self.place(f, &inner))
            }
            Rvalue::Ref { place, .. } => Expr::compound(format!("&{}", self.place(f, place))),
            Rvalue::Not(op) => Expr::compound(format!("!{}", self.operand(f, op).operand())),
            Rvalue::Array { ty, items } => {
                let items: Vec<String> = items.iter().map(|op| self.operand(f, op).text).collect();
                let items = if items.is_empty() {
                    "0".into()
                } else {
                    format!("{{{}}}", items.join(", "))
                };
                Expr::atom(format!("({}){{{items}}}", self.ty(*ty)))
            }
            // Anything other than all zeroes is a loop, written by `block`.
            Rvalue::Repeat { ty, operand } => {
                debug_assert!(is_zero(operand), "only zeroes repeat in an expression");
                Expr::atom(format!("({}){{0}}", self.ty(*ty)))
            }
            Rvalue::Range { ty, start, end } => {
                let (start, end) = (self.operand(f, start).text, self.operand(f, end).text);
                Expr::atom(format!("({}){{{start}, {end}}}", self.ty(*ty)))
            }
            Rvalue::MakeSlice { ty, start, len } => {
                let (start, len) = (self.operand(f, start).text, self.operand(f, len).text);
                Expr::atom(format!("({}){{{start}, {len}}}", self.ty(*ty)))
            }
            Rvalue::Cast { operand, ty } => {
                let from = self.operand_ty(f, operand);
                let value = self.operand(f, operand);
                let viewed = |t: TypeId| match self.mir.types.get(t) {
                    Ty::Ref { to, .. } => Some(self.mir.types.get(*to)),
                    _ => None,
                };
                match (viewed(from), viewed(*ty)) {
                    // A reference to an array becomes where its elements start, and how many.
                    (Some(Ty::Array { len, .. }), Some(Ty::Slice(_))) => Expr::atom(format!(
                        "({}){{{}->items, {}}}",
                        self.ty(*ty),
                        value.operand(),
                        self.mir.types.len(*len)
                    )),
                    // `[mut T]` to `[T]`: the same struct.
                    (Some(Ty::Slice(_)), Some(Ty::Slice(_))) => value,
                    // Text as its bytes: the same pointer and count, in a slice's struct.
                    (Some(Ty::Str), Some(Ty::Slice(_))) => {
                        let text = value.operand();
                        Expr::atom(format!("({}){{{text}.ptr, {text}.len}}", self.ty(*ty)))
                    }
                    _ => match (self.mir.types.get(from), self.mir.types.get(*ty)) {
                        (Ty::Float(_), Ty::Int(int)) => self.float_to_int(&value.operand(), *int),
                        _ => Expr::compound(format!("({}){}", self.ty(*ty), value.operand())),
                    },
                }
            }
            Rvalue::Some(op) => {
                let ty = self.operand_ty(f, op);
                let optional = (self.mir.types.lookup(&Ty::Optional(ty)))
                    .expect("validated MIR interns the optional types it builds");
                let value = self.operand(f, op).text;
                Expr::atom(format!("({}){{true, {value}}}", self.ty(optional)))
            }
            Rvalue::Binary { op, lhs, rhs } => {
                let pointer =
                    *op == BinOp::Add && self.operand_ty(f, lhs) == self.mir.types.pointer;
                let (l, r) = (self.operand(f, lhs), self.operand(f, rhs));
                // `ptr + n` offsets by `n` bytes.
                if pointer {
                    let (l, r) = (l.operand(), r.operand());
                    return Expr::compound(format!("(void*)((uint8_t*){l} + {r})"));
                }
                let (l, r) = (l.operand_of(*op, true), r.operand_of(*op, false));
                let symbol = match op {
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                    op => op.symbol(),
                };
                Expr {
                    text: format!("{l} {symbol} {r}"),
                    prec: Prec::of(*op),
                }
            }
            Rvalue::Struct {
                ty,
                variant: Some(variant),
                fields,
            } => {
                // `(struct Shape){ .tag = 1, .as.Rect = { w, h } }`
                let Ty::Struct(def, args) = self.mir.types.get(*ty) else {
                    unreachable!("variants are of enums")
                };
                // Each field by its designator: `.base = 1`, `.as.Rect.w = 2`.
                let s = self.mir.struct_def(*def, args);
                let mut text = format!("({}){{ .tag = {variant}", self.ty(*ty));
                for (&index, op) in s.variant_fields(*variant).iter().zip(fields) {
                    let member = &self.names.fields[def][index];
                    text.push_str(&format!(", .{member} = {}", self.operand(f, op).text));
                }
                text.push_str(" }");
                Expr::atom(text)
            }
            Rvalue::Struct { ty, fields, .. } => {
                let fields: Vec<String> =
                    fields.iter().map(|op| self.operand(f, op).text).collect();
                let fields = if fields.is_empty() {
                    "0".into()
                } else {
                    fields.join(", ")
                };
                Expr::atom(format!("({}){{{fields}}}", self.ty(*ty)))
            }
            Rvalue::Intrinsic {
                intrinsic,
                args,
                ty,
            } => {
                let p = self.operand(f, &args[0]).operand();
                let element = match self.mir.types.get(self.operand_ty(f, &args[0])) {
                    Ty::Ptr(element) => self.ty(*element),
                    _ => unreachable!("intrinsics work on typed pointers"),
                };
                let mut arg = |i: usize| self.operand(f, &args[i]).operand();
                match intrinsic {
                    Intrinsic::Resize => {
                        let count = arg(1);
                        Expr::compound(format!(
                            "({})__builtin_realloc({p}, {count} * sizeof({element}))",
                            self.ty(*ty)
                        ))
                    }
                    Intrinsic::Free => Expr::atom(format!("(__builtin_free({p}), 0)")),
                    Intrinsic::Read => Expr::atom(format!("{p}[{}]", arg(1))),
                    Intrinsic::Write => {
                        let (i, value) = (arg(1), arg(2));
                        Expr::atom(format!("({p}[{i}] = {value}, 0)"))
                    }
                    Intrinsic::Offset => Expr::compound(format!("{p} + {}", arg(1))),
                    Intrinsic::View => {
                        let len = arg(1);
                        Expr::atom(format!("({}){{{p}, {len}}}", self.ty(*ty)))
                    }
                    Intrinsic::IsNull => Expr::compound(format!("{p} == 0")),
                }
            }
            Rvalue::Call { func, args } => {
                let args: Vec<String> = args.iter().map(|a| self.operand(f, a).text).collect();
                Expr::atom(format!("{}({})", self.func_name(*func), args.join(", ")))
            }
        }
    }

    /// A float converted to the integer type `int`: C leaves one out of range undefined, so
    /// it saturates, and NaN is 0. `x` is read more than once, which only ever means reading a
    /// place again: calls aren't folded into conversions.
    fn float_to_int(&self, x: &str, int: IntTy) -> Expr {
        let target = match int {
            IntTy::I8 => "int8_t",
            IntTy::I16 => "int16_t",
            IntTy::I32 => "int32_t",
            IntTy::I64 => "int64_t",
            IntTy::ISize => "intptr_t",
            IntTy::U8 => "uint8_t",
            IntTy::U16 => "uint16_t",
            IntTy::U32 => "uint32_t",
            IntTy::U64 => "uint64_t",
            IntTy::USize => "uintptr_t",
            // C can't write their limits as literals; these convert unchecked.
            IntTy::I128 => return Expr::compound(format!("(__int128){x}")),
            IntTy::U128 => return Expr::compound(format!("(unsigned __int128){x}")),
        };
        let (min, max) = match int {
            IntTy::I8 => ("INT8_MIN", "INT8_MAX"),
            IntTy::I16 => ("INT16_MIN", "INT16_MAX"),
            IntTy::I32 => ("INT32_MIN", "INT32_MAX"),
            IntTy::I64 => ("INT64_MIN", "INT64_MAX"),
            IntTy::ISize => ("INTPTR_MIN", "INTPTR_MAX"),
            IntTy::U8 => ("0", "UINT8_MAX"),
            IntTy::U16 => ("0", "UINT16_MAX"),
            IntTy::U32 => ("0", "UINT32_MAX"),
            IntTy::U64 => ("0", "UINT64_MAX"),
            IntTy::USize => ("0", "UINTPTR_MAX"),
            IntTy::I128 | IntTy::U128 => unreachable!("handled above"),
        };
        // Comparing in `double`: a limit that rounds up there still compares right, as anything
        // that large or larger is out of range.
        Expr::compound(format!(
            "({x} != {x} ? ({target})0 : {x} <= (double){min} ? ({target}){min} \
             : {x} >= (double){max} ? ({target}){max} : ({target}){x})"
        ))
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
        (self.mir.project(ty, projection)).expect("validated MIR has well-typed places")
    }

    fn operand(&self, f: &mut FnState, op: &Operand) -> Expr {
        match op {
            // A temporary computed by the previous statement is folded in here.
            Operand::Copy(place) | Operand::Move(place) if place.projection.is_empty() => {
                match f.fold(place.local) {
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
                // A `cstr` is the literal; a `str` is where it starts, and how many bytes it has.
                ConstKind::Str(s) if c.ty == self.mir.types.cstr => {
                    format!("(uint8_t*){}", c_string(s))
                }
                ConstKind::Str(s) => format!(
                    "({}){{(uint8_t*){}, {}}}",
                    self.ty(c.ty),
                    c_string(s),
                    s.len()
                ),
                ConstKind::Unit => "0".into(),
                ConstKind::Func(id) => format!("(void*)&{}", self.func_name(*id)),
                // A compound literal zeroes anything: `(void*){0}`, `(Color){0}`.
                ConstKind::Zeroed => format!("({}){{0}}", self.ty(c.ty)),
            }),
        }
    }

    /// A place as a C lvalue: `x`, `p.x`, `self->x`, `(*r)`.
    fn place(&self, f: &FnState, place: &Place) -> String {
        f.mention(place.local);
        let mut s = f.locals[place.local.id].clone();
        let mut ty = f.body.local(place.local).ty;
        let mut projections = place.projection.iter().peekable();
        while let Some(&projection) = projections.next() {
            // `(*p).x` is `p->x`, and `(*p)[i]` is `p->items[i]`.
            if projection == Projection::Deref
                && let Some(&&next @ (Projection::Field(_) | Projection::Index(_))) =
                    projections.peek()
            {
                projections.next();
                let viewed = self.project(ty, Projection::Deref);
                s = match (self.mir.types.get(viewed), next) {
                    // `(*s)[i]` of a slice is `s.ptr[i]`.
                    (Ty::Slice(_) | Ty::Str, Projection::Index(index)) => {
                        format!("{s}.ptr[{}]", self.index(f, index))
                    }
                    _ => format!("{s}->{}", self.member(f, viewed, next)),
                };
                ty = self.project(viewed, next);
                continue;
            }
            s = match projection {
                Projection::Field(_) | Projection::Index(_) => {
                    format!("{s}.{}", self.member(f, ty, projection))
                }
                Projection::Deref => format!("(*{s})"),
            };
            ty = self.project(ty, projection);
        }
        s
    }

    /// What follows the `.` or `->` for a field, or an array's element: `x`, `items[i]`.
    fn member(&self, f: &FnState, ty: TypeId, projection: Projection) -> String {
        match projection {
            Projection::Field(i) => self.field_name(ty, i).to_string(),
            Projection::Index(index) => format!("items[{}]", self.index(f, index)),
            Projection::Deref => unreachable!("a dereference isn't a member"),
        }
    }

    /// An array index: the local holding it, or the temporary folded in.
    fn index(&self, f: &FnState, index: Local) -> String {
        match f.fold(index) {
            Some(expr) => expr.text,
            None => {
                f.mention(index);
                f.locals[index.id].clone()
            }
        }
    }

    fn field_name(&self, ty: TypeId, index: usize) -> &str {
        match self.mir.types.get(ty) {
            Ty::Struct(def, _) => &self.names.fields[def][index],
            Ty::Optional(_) if index == 0 => "some",
            Ty::Optional(_) => "value",
            Ty::Range(_) if index == 0 => "start",
            Ty::Range(_) => "end",
            Ty::Ref { .. } if index == 0 => "ptr",
            Ty::Ref { .. } => "len",
            _ => unreachable!("fields belong to structs and optionals"),
        }
    }
}

/// Whether `op` is a constant whose bytes are all zero, so `{0}` makes copies of it.
fn is_zero(op: &Operand) -> bool {
    match op {
        Operand::Const(c) => match &c.kind {
            ConstKind::Int(n) => *n == 0,
            ConstKind::Float(x) => *x == 0.0 && x.is_sign_positive(),
            ConstKind::Bool(b) => !b,
            ConstKind::Unit | ConstKind::Zeroed => true,
            ConstKind::Str(_) | ConstKind::Func(_) => false,
        },
        _ => false,
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

/// The places an rvalue reads, or takes the address of.
fn places(rvalue: &Rvalue) -> Vec<&Place> {
    let ops: Vec<&Operand> = match rvalue {
        Rvalue::Use(op)
        | Rvalue::Not(op)
        | Rvalue::Some(op)
        | Rvalue::Repeat { operand: op, .. }
        | Rvalue::Cast { operand: op, .. } => vec![op],
        Rvalue::Binary { lhs, rhs, .. }
        | Rvalue::Range {
            start: lhs,
            end: rhs,
            ..
        }
        | Rvalue::MakeSlice {
            start: lhs,
            len: rhs,
            ..
        } => vec![lhs, rhs],
        Rvalue::Struct { fields: ops, .. }
        | Rvalue::Array { items: ops, .. }
        | Rvalue::Call { args: ops, .. }
        | Rvalue::Intrinsic { args: ops, .. } => ops.iter().collect(),
        Rvalue::Ref { place, .. } => return vec![place],
    };
    ops.into_iter()
        .filter_map(|op| match op {
            Operand::Copy(place) | Operand::Move(place) => Some(place),
            Operand::Const(_) => None,
        })
        .collect()
}

/// The locals `dest = rvalue` could fold pending temporaries into: whole operands, and
/// indexes.
fn statement_reads(dest: &Place, rvalue: &Rvalue) -> Vec<Local> {
    let mut read: Vec<Local> = dest.index_locals().collect();
    for place in places(rvalue) {
        if place.projection.is_empty() && !matches!(rvalue, Rvalue::Ref { .. }) {
            read.push(place.local);
        }
        read.extend(place.index_locals());
    }
    read
}

/// The locals whose address is taken somewhere in `body`.
fn address_taken(body: &Body) -> HashSet<Local> {
    let mut taken = HashSet::new();
    for stmt in body.blocks.iter().flat_map(|b| &b.stmts) {
        match &stmt.kind {
            StatementKind::Assign(_, Rvalue::Ref { place, .. }) => {
                taken.insert(place.local);
            }
            StatementKind::Drop { place, .. } => {
                taken.insert(place.local);
            }
            _ => {}
        }
    }
    taken
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

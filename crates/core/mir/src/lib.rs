//! Each function as a control-flow graph. Operands say whether they copy or move, so
//! ownership is explicit; generic functions are monomorphized, so types are concrete.

pub mod dataflow;
mod display;
mod validate;

pub use som_ast::BinOp;
pub use som_resolve::DefId;
pub use som_typeck::{FloatTy, IntTy, Ty, TypeId, TypeTable, Value};

pub use som_base::{Arena, Id};
use som_base::{IdPrefix, Span};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct Mir {
    pub types: TypeTable,
    pub funcs: Arena<Function>,
    pub structs: Vec<StructDef>,
    /// The types implementing `Drop` themselves: what's in them can't be moved out.
    pub drop_types: HashSet<TypeId>,
    /// Called by `@fail` and `@unreachable`: tells the `@on fail` functions, then stops.
    pub fail: Option<FuncId>,
    /// Every `@const` initializer and `@assert(…);`, run while compiling whether used or not.
    pub checks: Vec<FuncId>,
    /// What `som test` starts at instead of `main`: each `@test`, then a summary. Nothing
    /// else calls it, so it's shaken out of the program otherwise.
    pub tests: Option<FuncId>,
    /// The `@vertex`, `@fragment` and `@compute` functions: shaders, which `gpu::shader(name)`
    /// and `gpu::kernel(name)` compile for the GPU. They're ordinary functions otherwise.
    pub stages: HashMap<FuncId, som_lang::Stage>,
    /// Each instance of the standard library's `shader` and `kernel`, which only work while
    /// compiling, and the shader it's for: `None` if what it's given isn't a function named
    /// where it's passed.
    pub wgsl: HashMap<FuncId, Option<FuncId>>,
    /// Each instance of `mem::size_of`, and the type it measures: given its body by the
    /// compile-time pass, which lays types out as C does.
    pub size_of: HashMap<FuncId, TypeId>,
    /// The copies of functions compiled for the GPU, and WGSL's own functions: only shaders
    /// use them, never the program on the CPU.
    pub gpu: HashSet<FuncId>,
    /// The structs the language gives meaning to, by which they are.
    pub lang_structs: HashMap<DefId, som_lang::LangStruct>,
    /// For `fn main() -> !`: what turns what it gives into the program's exit code, saying
    /// why it failed if it did.
    pub main_exit: Option<FuncId>,
    struct_index: HashMap<(DefId, Vec<TypeId>), usize>,
}

impl Mir {
    pub fn new(types: TypeTable) -> Self {
        Mir {
            types,
            funcs: Arena::new(),
            structs: Vec::new(),
            drop_types: HashSet::new(),
            fail: None,
            checks: Vec::new(),
            tests: None,
            stages: HashMap::new(),
            wgsl: HashMap::new(),
            size_of: HashMap::new(),
            lang_structs: HashMap::new(),
            gpu: HashSet::new(),
            main_exit: None,
            struct_index: HashMap::new(),
        }
    }

    pub fn add_struct(&mut self, def: StructDef) {
        let key = (def.def, def.args.clone());
        self.struct_index.insert(key, self.structs.len());
        self.structs.push(def);
    }

    pub fn has_struct(&self, def: DefId, args: &[TypeId]) -> bool {
        self.struct_index.contains_key(&(def, args.to_vec()))
    }

    pub fn retain_structs(&mut self, keep: impl Fn(&StructDef) -> bool) {
        self.structs.retain(keep);
        self.struct_index = self
            .structs
            .iter()
            .enumerate()
            .map(|(i, s)| ((s.def, s.args.clone()), i))
            .collect();
    }

    pub fn struct_def(&self, def: DefId, args: &[TypeId]) -> &StructDef {
        let Some(&index) = self.struct_index.get(&(def, args.to_vec())) else {
            let args: Vec<String> = args
                .iter()
                .map(|&a| format!("{:?}", self.types.get(a)))
                .collect();
            panic!(
                "no copy of struct {def:?} for type arguments [{}]",
                args.join(", ")
            );
        };
        &self.structs[index]
    }

    pub fn project(&self, ty: TypeId, projection: Projection) -> Option<TypeId> {
        match (projection, self.types.get(ty)) {
            (Projection::Deref, Ty::Ref { to, .. } | Ty::Ptr(to)) => Some(*to),
            (Projection::Field(i), Ty::Struct(def, args)) => {
                self.struct_def(*def, args).fields.get(i).map(|&(_, ty)| ty)
            }
            (Projection::Field(0), Ty::Optional(_)) => Some(self.types.bool),
            (Projection::Field(1), Ty::Optional(inner)) => Some(*inner),
            (Projection::Index(_), Ty::Array { element, .. } | Ty::Slice(element)) => {
                Some(*element)
            }
            (Projection::Index(_), Ty::Str) => Some(self.types.u8),
            (Projection::Field(0 | 1), Ty::Range(int)) => Some(*int),
            (Projection::Field(0), Ty::Ref { to, .. })
                if matches!(self.types.get(*to), Ty::Slice(_) | Ty::Str) =>
            {
                Some(self.types.pointer)
            }
            (Projection::Field(1), Ty::Ref { to, .. })
                if matches!(self.types.get(*to), Ty::Slice(_) | Ty::Str) =>
            {
                self.types.lookup(&Ty::Int(IntTy::USize))
            }
            _ => None,
        }
    }

    pub fn place_ty(&self, locals: &Arena<LocalDecl>, place: &Place) -> TypeId {
        (place.projection.iter()).fold(locals[place.local].ty, |ty, &projection| {
            (self.project(ty, projection)).expect("validated MIR has well-typed places")
        })
    }

    pub fn operand_ty(&self, locals: &Arena<LocalDecl>, operand: &Operand) -> TypeId {
        match operand {
            Operand::Const(c) => c.ty,
            Operand::Copy(place) | Operand::Move(place) => self.place_ty(locals, place),
        }
    }

    pub fn func(&self, id: FuncId) -> &Function {
        &self.funcs[id]
    }

    /// A `&mut` moves: two copies of it would be two ways to change the same thing.
    pub fn is_copy(&self, ty: TypeId) -> bool {
        match self.types.get(ty) {
            Ty::Struct(def, args) => self.struct_def(*def, args).copy,
            Ty::Ref { mutable: true, .. } => false,
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(*inner),
            _ => true,
        }
    }
}

/// The name of an enum's first field, saying which variant it is.
pub const ENUM_TAG: &str = "tag";

/// Ids stay the same when other functions are removed.
pub type FuncId = Id<Function>;

pub type BlockId = Id<Block>;

pub type Local = Id<LocalDecl>;

pub const RETURN: Local = Id::new(0);

impl IdPrefix for Block {
    const PREFIX: &'static str = "bb";
}

impl IdPrefix for LocalDecl {
    const PREFIX: &'static str = "_";
}

/// A struct, or one copy of a generic struct per set of type arguments.
pub struct StructDef {
    pub def: DefId,
    pub args: Vec<TypeId>,
    pub name: String,
    /// An enum's are its `u32` `tag`, then its common fields, then each variant's own fields
    /// (named like `Circle.0`, `Rect.w`), of which only the tag's variant's hold anything.
    pub fields: Vec<(String, TypeId)>,
    pub variants: Option<Vec<VariantDef>>,
    pub copy: bool,
    /// An `extern struct`: values are pointers to a struct only C sees inside.
    pub handle: bool,
}

impl StructDef {
    pub fn variant_fields(&self, variant: usize) -> &[usize] {
        &self.variants.as_ref().expect("only enums have variants")[variant].fields
    }
}

pub struct VariantDef {
    pub name: String,
    /// Indexes into the enum's `fields`: the common ones, then its own.
    pub fields: Vec<usize>,
}

pub struct Function {
    pub name: String,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub span: Span,
    /// `None` for extern declarations.
    pub body: Option<Body>,
    pub link_name: Option<String>,
    pub library: Option<PathBuf>,
    pub is_entry: bool,
    pub variadic: bool,
}

pub struct Body {
    pub locals: Arena<LocalDecl>,
    pub arg_count: usize,
    /// Pointers the function must assign through on every path before it returns, and
    /// before reading through them.
    pub outs: Vec<Local>,
    /// Temporaries holding a method call's `&mut` of its receiver: until the call uses it,
    /// it's only a shared borrow, so `v.push(v.len())` can still read `v`.
    pub two_phase: Vec<Local>,
    /// The entry block is `bb0`.
    pub blocks: Arena<Block>,
}

impl Body {
    pub fn operands(&self) -> Vec<&Operand> {
        let mut found = Vec::new();
        for block in self.blocks.iter() {
            for statement in &block.stmts {
                if let StatementKind::Assign(_, rvalue) = &statement.kind {
                    found.extend(rvalue.operands());
                }
            }
            if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
                found.push(condition);
            }
        }
        found
    }

    pub fn operands_mut(&mut self) -> Vec<&mut Operand> {
        let mut found = Vec::new();
        for block in self.blocks.iter_mut() {
            for statement in &mut block.stmts {
                if let StatementKind::Assign(_, rvalue) = &mut statement.kind {
                    found.extend(rvalue.operands_mut());
                }
            }
            if let TerminatorKind::Branch { condition, .. } = &mut block.terminator.kind {
                found.push(condition);
            }
        }
        found
    }

    pub fn local(&self, local: Local) -> &LocalDecl {
        &self.locals[local]
    }

    pub fn block(&self, block: BlockId) -> &Block {
        &self.blocks[block]
    }

    pub fn args(&self) -> impl Iterator<Item = Local> {
        (1..=self.arg_count).map(Id::new)
    }

    /// `a` is `b`, and `b` is `a`: their declarations, and everywhere they're mentioned.
    pub fn swap_locals(&mut self, a: Local, b: Local) {
        let swap = |local: &mut Local| {
            if *local == a {
                *local = b;
            } else if *local == b {
                *local = a;
            }
        };
        let place = |place: &mut Place| {
            swap(&mut place.local);
            for projection in &mut place.projection {
                if let Projection::Index(index) = projection {
                    swap(index);
                }
            }
        };
        for block in self.blocks.iter_mut() {
            for statement in &mut block.stmts {
                for (read, _) in &mut statement.reads {
                    place(read);
                }
                match &mut statement.kind {
                    StatementKind::Assign(to, rvalue) => {
                        place(to);
                        if let Rvalue::Ref {
                            place: borrowed, ..
                        } = rvalue
                        {
                            place(borrowed);
                        }
                        for operand in rvalue.operands_mut() {
                            if let Operand::Copy(p) | Operand::Move(p) = operand {
                                place(p);
                            }
                        }
                    }
                    StatementKind::StorageLive(local) | StatementKind::StorageDead(local) => {
                        swap(local)
                    }
                    StatementKind::Drop {
                        place: dropped,
                        flag,
                        ..
                    } => {
                        place(dropped);
                        if let Some(flag) = flag {
                            swap(flag);
                        }
                    }
                }
            }
            if let TerminatorKind::Branch {
                condition: Operand::Copy(p) | Operand::Move(p),
                ..
            } = &mut block.terminator.kind
            {
                place(p);
            }
        }
        self.two_phase.iter_mut().for_each(swap);
        self.outs.iter_mut().for_each(swap);
        self.locals.swap(a, b);
    }
}

pub struct LocalDecl {
    pub ty: TypeId,
    /// Temporaries have none.
    pub name: Option<String>,
    pub mutable: bool,
    pub span: Span,
}

pub struct Block {
    pub stmts: Vec<Statement>,
    pub terminator: Terminator,
}

pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
    /// Where each place among its operands is read, in operand order, so diagnostics can
    /// point at `p.age` in `f(p.age)` rather than the whole statement.
    pub reads: Vec<(Place, Span)>,
}

pub enum StatementKind {
    Assign(Place, Rvalue),
    StorageLive(Local),
    StorageDead(Local),
    /// Calls `glue` with a reference to `place`. With `flag`, only if that local is `true`:
    /// the value was moved on some paths here and not on others.
    Drop {
        place: Place,
        glue: FuncId,
        flag: Option<Local>,
    },
}

pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: Span,
}

pub enum TerminatorKind {
    Goto(BlockId),
    Branch {
        condition: Operand,
        then: BlockId,
        otherwise: BlockId,
    },
    Return,
    Unreachable,
}

impl TerminatorKind {
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            TerminatorKind::Goto(b) => vec![*b],
            TerminatorKind::Branch {
                then, otherwise, ..
            } => vec![*then, *otherwise],
            TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<Projection>,
}

impl Place {
    pub fn local(local: Local) -> Place {
        Place {
            local,
            projection: Vec::new(),
        }
    }

    pub fn project(mut self, projection: Projection) -> Place {
        self.projection.push(projection);
        self
    }

    /// Its own local, then those holding indexes.
    pub fn locals(&self) -> impl Iterator<Item = Local> + '_ {
        std::iter::once(self.local).chain(self.projection.iter().filter_map(|p| match p {
            Projection::Index(index) => Some(*index),
            _ => None,
        }))
    }

    pub fn index_locals(&self) -> impl Iterator<Item = Local> + '_ {
        self.locals().skip(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Projection {
    /// An optional's `.0` is whether it holds a value and `.1` the value; a slice
    /// reference's `.0` is where its elements start and `.1` how many there are.
    Field(usize),
    Deref,
    /// The index, a `usize`, has already been checked to be in bounds.
    Index(Local),
}

pub enum Rvalue {
    Use(Operand),
    Ref {
        mutable: bool,
        place: Place,
    },
    Binary {
        op: BinOp,
        lhs: Operand,
        rhs: Operand,
    },
    Not(Operand),
    /// An optional without a value is `Zeroed`.
    Some(Operand),
    Array {
        ty: TypeId,
        items: Vec<Operand>,
    },
    Repeat {
        ty: TypeId,
        operand: Operand,
    },
    Range {
        ty: TypeId,
        start: Operand,
        end: Operand,
    },
    MakeSlice {
        ty: TypeId,
        start: Operand,
        len: Operand,
    },
    /// An integer wraps if it doesn't fit; a float becomes the integer nearest zero,
    /// saturating. Also turns a reference to an array or `[mut T]` into a slice.
    Cast {
        operand: Operand,
        ty: TypeId,
    },
    /// With `variant`, an enum with that variant's fields and its tag set.
    Struct {
        ty: TypeId,
        variant: Option<usize>,
        fields: Vec<Operand>,
    },
    Call {
        func: FuncId,
        args: Vec<Operand>,
    },
    /// Through a function's address, a `fn(…) -> …`: `ops[0](5)`.
    CallIndirect {
        callee: Operand,
        args: Vec<Operand>,
    },
    /// Raw memory through a `*T`; nothing checks it. It's what `List` is built on.
    Intrinsic {
        intrinsic: Intrinsic,
        args: Vec<Operand>,
        ty: TypeId,
    },
}

impl Rvalue {
    /// Whether it calls a function, named or through its address.
    pub fn is_call(&self) -> bool {
        matches!(self, Rvalue::Call { .. } | Rvalue::CallIndirect { .. })
    }

    pub fn operands(&self) -> Vec<&Operand> {
        match self {
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
            Rvalue::CallIndirect { callee, args } => std::iter::once(callee).chain(args).collect(),
            Rvalue::Ref { .. } => Vec::new(),
        }
    }

    pub fn operands_mut(&mut self) -> Vec<&mut Operand> {
        match self {
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
            | Rvalue::Intrinsic { args: ops, .. } => ops.iter_mut().collect(),
            Rvalue::CallIndirect { callee, args } => {
                std::iter::once(callee).chain(args.iter_mut()).collect()
            }
            Rvalue::Ref { .. } => Vec::new(),
        }
    }

    pub fn places(&self) -> Vec<&Place> {
        match self {
            Rvalue::Ref { place, .. } => vec![place],
            _ => self
                .operands()
                .into_iter()
                .filter_map(Operand::place)
                .collect(),
        }
    }

    pub fn moves(&self) -> Vec<&Place> {
        (self.operands().into_iter())
            .filter_map(|op| match op {
                Operand::Move(place) => Some(place),
                _ => None,
            })
            .collect()
    }
}

/// What an `Rvalue::Intrinsic` does, given its arguments: a pointer `p` to `T`s, then the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    /// `p, count`: room for `count` `T`s, moved from where `p` points (or new, if it's null).
    Resize,
    /// `p`: gives back what `p` points to.
    Free,
    /// `p, i`: the `T` at `i`, moved or copied out.
    Take,
    /// `p, i, value`: stores `value` at `i`, without reading what was there.
    Put,
    /// `p, i`: where the `T` at `i` is, as a reference.
    Offset,
    /// `p, len`: a slice of the `len` `T`s starting at `p`.
    View,
    /// `p`: whether it's null.
    IsNull,
}

impl Intrinsic {
    /// Changes memory, so it has to happen even if its result isn't used.
    pub fn has_effect(self) -> bool {
        matches!(self, Intrinsic::Resize | Intrinsic::Free | Intrinsic::Put)
    }
}

#[derive(Clone)]
pub enum Operand {
    Copy(Place),
    /// The place may not be used again until it's reassigned.
    Move(Place),
    Const(Const),
}

impl Operand {
    pub fn place(&self) -> Option<&Place> {
        match self {
            Operand::Copy(place) | Operand::Move(place) => Some(place),
            Operand::Const(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct Const {
    pub kind: ConstKind,
    pub ty: TypeId,
}

#[derive(Clone)]
pub enum ConstKind {
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    Func(FuncId),
    /// Every byte zero: `0`, `false`, a null pointer, or a struct of those.
    Zeroed,
    /// A struct, enum, array, optional or range: its parts in the order `Projection`s reach
    /// them.
    Aggregate(Vec<Const>),
    /// A `@const`'s initializer, which `som_comptime` runs and replaces with its value.
    Pending(FuncId),
}

//! MIR: each function as a control-flow graph of basic blocks.
//!
//! Every value lives in a numbered local (`_0` is the return value, `_1..=_n` the
//! parameters, then `let` bindings and temporaries). Statements assign to
//! *places*: a local with field and dereference projections. Operands say
//! whether they copy or move, so ownership is explicit. Generic functions are
//! monomorphized: MIR only contains concrete types.

mod display;
mod validate;

pub use som_ast::BinOp;
pub use som_resolve::DefId;
pub use som_typeck::{FloatTy, IntTy, Ty, TypeId, TypeTable};

pub use som_base::{Arena, Id};
use som_base::{IdPrefix, Span};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Mir {
    pub types: TypeTable,
    pub funcs: Arena<Function>,
    pub structs: Vec<StructDef>,
    struct_index: HashMap<(DefId, Vec<TypeId>), usize>,
}

impl Mir {
    pub fn new(types: TypeTable) -> Self {
        Mir {
            types,
            funcs: Arena::new(),
            structs: Vec::new(),
            struct_index: HashMap::new(),
        }
    }

    pub fn add_struct(&mut self, def: StructDef) {
        let key = (def.def, def.args.clone());
        self.struct_index.insert(key, self.structs.len());
        self.structs.push(def);
    }

    /// Whether the struct `def` with type arguments `args` has been added.
    pub fn has_struct(&self, def: DefId, args: &[TypeId]) -> bool {
        self.struct_index.contains_key(&(def, args.to_vec()))
    }

    /// Keeps only the structs `keep` returns true for.
    pub fn retain_structs(&mut self, keep: impl Fn(&StructDef) -> bool) {
        self.structs.retain(keep);
        self.struct_index = self
            .structs
            .iter()
            .enumerate()
            .map(|(i, s)| ((s.def, s.args.clone()), i))
            .collect();
    }

    /// The struct a `Ty::Struct(def, args)` refers to: one copy per set of type arguments.
    pub fn struct_def(&self, def: DefId, args: &[TypeId]) -> &StructDef {
        &self.structs[self.struct_index[&(def, args.to_vec())]]
    }

    /// The type of `projection` applied to a place of type `ty`, or `None` if it doesn't apply.
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
            // A range's two ends.
            (Projection::Field(0 | 1), Ty::Range(int)) => Some(*int),
            // A slice's reference holds where its elements start, and how many there are.
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

    pub fn func(&self, id: FuncId) -> &Function {
        &self.funcs[id]
    }

    /// Whether values of `ty` are copied rather than moved. A `&mut` moves: two copies of
    /// it would be two ways to change the same thing.
    pub fn is_copy(&self, ty: TypeId) -> bool {
        match self.types.get(ty) {
            Ty::Struct(def, args) => self.struct_def(*def, args).copy,
            Ty::Ref { mutable: true, .. } => false,
            // As copyable as what it holds.
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(*inner),
            _ => true,
        }
    }
}

/// A function, by its id in `Mir::funcs`. Ids stay the same when other functions are removed.
pub type FuncId = Id<Function>;

/// A block, by its id in `Body::blocks`. Written `bb3`.
pub type BlockId = Id<Block>;

/// A local, by its id in `Body::locals`. Written `_3`.
pub type Local = Id<LocalDecl>;

/// The local holding a function's return value: `_0`.
pub const RETURN: Local = Id::new(0);

impl IdPrefix for Block {
    const PREFIX: &'static str = "bb";
}

impl IdPrefix for LocalDecl {
    const PREFIX: &'static str = "_";
}

/// A struct, or one copy of a generic struct for a set of type arguments.
pub struct StructDef {
    pub def: DefId,
    /// Its type arguments, if it's generic: `[i32]` for `Pair<i32>`.
    pub args: Vec<TypeId>,
    /// Readable name, with its type arguments: `geo::Point`, `Pair<i32>`.
    pub name: String,
    /// Each field's name and type, with the type arguments filled in. An enum's first field
    /// is its `tag`, a `u32` saying which variant it is; then its common fields, which every
    /// variant has (`base`); then each variant's own fields, one variant after another, named
    /// like `Circle.0` and `Rect.w`. Only the tag's variant's own fields hold anything.
    pub fields: Vec<(String, TypeId)>,
    /// For an enum: its variants, in order.
    pub variants: Option<Vec<VariantDef>>,
    /// Values are copied rather than moved.
    pub copy: bool,
    /// An `extern struct`: values are pointers to a struct only C sees inside.
    pub handle: bool,
}

impl StructDef {
    /// The fields of variant `variant`, in its order, by their index in `fields`.
    pub fn variant_fields(&self, variant: usize) -> &[usize] {
        &self.variants.as_ref().expect("only enums have variants")[variant].fields
    }
}

/// One of an enum's variants.
pub struct VariantDef {
    pub name: String,
    /// Its fields, in order, by their index in the enum's `fields`: common ones, then its own.
    pub fields: Vec<usize>,
}

pub struct Function {
    /// Readable name, qualified by module and struct: `geo::Point::new`, `id<i32>`.
    pub name: String,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
    pub span: Span,
    /// `Some` for functions defined in som, `None` for extern declarations.
    pub body: Option<Body>,
    /// For extern functions: the symbol to link against.
    pub link_name: Option<String>,
    /// For extern functions from an `extern "lib"` block: the library file to link.
    pub library: Option<PathBuf>,
    /// The program's entry point: `main` in the root module.
    pub is_entry: bool,
    /// An extern taking extra arguments after `params`, like C's `printf`.
    pub variadic: bool,
}

pub struct Body {
    pub locals: Arena<LocalDecl>,
    pub arg_count: usize,
    /// The `out` parameters: pointers the function must assign through on every path
    /// before it returns, and before reading through them.
    pub outs: Vec<Local>,
    /// The entry block is `bb0`.
    pub blocks: Arena<Block>,
}

impl Body {
    pub fn local(&self, local: Local) -> &LocalDecl {
        &self.locals[local]
    }

    pub fn block(&self, block: BlockId) -> &Block {
        &self.blocks[block]
    }

    /// The parameters' locals: `_1..=_n`.
    pub fn args(&self) -> impl Iterator<Item = Local> {
        (1..=self.arg_count).map(Id::new)
    }
}

pub struct LocalDecl {
    pub ty: TypeId,
    /// The source name, for parameters and `let` bindings. Temporaries have none.
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
}

pub enum StatementKind {
    Assign(Place, Rvalue),
    /// The local's scope starts: it may be assigned from here on.
    StorageLive(Local),
    /// The local's scope ends: it, and anything borrowed from it, is gone.
    StorageDead(Local),
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
    /// Control never gets here, e.g. after an infinite `loop`.
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

/// A memory location: a local, then a path through fields and dereferences.
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

    /// Every local the place reads: its own, and those holding indexes.
    pub fn locals(&self) -> impl Iterator<Item = Local> + '_ {
        std::iter::once(self.local).chain(self.projection.iter().filter_map(|p| match p {
            Projection::Index(index) => Some(*index),
            _ => None,
        }))
    }

    /// The locals holding the place's indexes.
    pub fn index_locals(&self) -> impl Iterator<Item = Local> + '_ {
        self.locals().skip(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Projection {
    /// `.n`: the nth field of a struct. An optional has two: `.0` is whether it holds a
    /// value, and `.1` the value. So does a reference to a slice: `.0` is where its elements
    /// start, and `.1` how many there are.
    Field(usize),
    /// `*`: what a reference points to.
    Deref,
    /// `[i]`: the element of an array at the index the local holds, a `usize`. The index has
    /// been checked to be in bounds.
    Index(Local),
}

pub enum Rvalue {
    Use(Operand),
    /// `&place` or `&mut place`.
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
    /// An optional holding a value. One without is `Zeroed`.
    Some(Operand),
    /// An array of type `ty`, with these elements.
    Array {
        ty: TypeId,
        items: Vec<Operand>,
    },
    /// An array of type `ty`, every element a copy of `operand`.
    Repeat {
        ty: TypeId,
        operand: Operand,
    },
    /// A range of type `ty`, from `start` up to `end`.
    Range {
        ty: TypeId,
        start: Operand,
        end: Operand,
    },
    /// A slice (or text) of type `ty`: `len` elements starting where the reference `start`
    /// points.
    MakeSlice {
        ty: TypeId,
        start: Operand,
        len: Operand,
    },
    /// A value converted to type `ty`: a number to another number type (an integer wraps if it
    /// doesn't fit, a float becomes the integer nearest zero, saturating); or a reference to an
    /// array, or a `[mut T]`, to a slice.
    Cast {
        operand: Operand,
        ty: TypeId,
    },
    /// A struct value, with fields in declaration order; or, with `variant`, an enum's, with
    /// that variant's fields in order and its tag set.
    Struct {
        /// The struct type, with its type arguments.
        ty: TypeId,
        variant: Option<usize>,
        fields: Vec<Operand>,
    },
    Call {
        func: FuncId,
        args: Vec<Operand>,
    },
    /// An operation on raw memory through a `*T`, with a result of type `ty`. Nothing checks
    /// it: it's what `List` is built on.
    Intrinsic {
        intrinsic: Intrinsic,
        args: Vec<Operand>,
        ty: TypeId,
    },
}

/// What an `Rvalue::Intrinsic` does, given its arguments: a pointer `p` to `T`s, then the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intrinsic {
    /// `p, count`: room for `count` `T`s, moved from where `p` points (or new, if it's null).
    Resize,
    /// `p`: gives back what `p` points to.
    Free,
    /// `p, i`: the `T` at `i`, moved or copied out.
    Read,
    /// `p, i, value`: stores `value` at `i`, without reading what was there.
    Write,
    /// `p, i`: where the `T` at `i` is, as a reference.
    Offset,
    /// `p, len`: a slice of the `len` `T`s starting at `p`.
    View,
    /// `p`: whether it's null.
    IsNull,
}

impl Intrinsic {
    /// Whether it changes memory, so it has to happen even if its result isn't used.
    pub fn has_effect(self) -> bool {
        matches!(self, Intrinsic::Resize | Intrinsic::Free | Intrinsic::Write)
    }
}

#[derive(Clone)]
pub enum Operand {
    Copy(Place),
    /// Like `Copy`, but the place may not be used again until it's reassigned.
    Move(Place),
    Const(Const),
}

#[derive(Clone)]
pub struct Const {
    pub kind: ConstKind,
    pub ty: TypeId,
}

#[derive(Clone)]
pub enum ConstKind {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Unit,
    /// A function used as a value.
    Func(FuncId),
    /// Every byte zero: `0`, `false`, a null pointer, or a struct of those.
    Zeroed,
}

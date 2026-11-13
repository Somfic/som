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

use som_base::Span;
use std::collections::HashMap;

pub struct Mir {
    pub types: TypeTable,
    pub funcs: Vec<Function>,
    pub structs: Vec<StructDef>,
    struct_index: HashMap<DefId, usize>,
}

impl Mir {
    pub fn new(types: TypeTable) -> Self {
        Mir {
            types,
            funcs: Vec::new(),
            structs: Vec::new(),
            struct_index: HashMap::new(),
        }
    }

    pub fn add_struct(&mut self, def: StructDef) {
        self.struct_index.insert(def.def, self.structs.len());
        self.structs.push(def);
    }

    /// The struct a `Ty::Struct(def)` refers to.
    pub fn struct_def(&self, def: DefId) -> &StructDef {
        &self.structs[self.struct_index[&def]]
    }

    pub fn func(&self, id: FuncId) -> &Function {
        &self.funcs[id.0 as usize]
    }

    /// Whether values of `ty` are copied rather than moved.
    pub fn is_copy(&self, ty: TypeId) -> bool {
        match self.types.get(ty) {
            Ty::Struct(def) => self.struct_def(*def).copy,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FuncId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Local(pub u32);

impl Local {
    pub const RETURN: Local = Local(0);
}

pub struct StructDef {
    pub def: DefId,
    pub name: String,
    pub fields: Vec<(String, TypeId)>,
    /// Values are copied rather than moved.
    pub copy: bool,
    /// An `extern struct`: values are pointers to a struct only C sees inside.
    pub handle: bool,
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
    /// The program's entry point: `main` in the root module.
    pub is_entry: bool,
    /// An extern taking extra arguments after `params`, like C's `printf`.
    pub variadic: bool,
}

pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub arg_count: usize,
    /// The `out` parameters: pointers the function must assign through on every path
    /// before it returns, and before reading through them.
    pub outs: Vec<Local>,
    pub blocks: Vec<Block>,
}

impl Body {
    pub fn local(&self, local: Local) -> &LocalDecl {
        &self.locals[local.0 as usize]
    }

    pub fn block(&self, block: BlockId) -> &Block {
        &self.blocks[block.0 as usize]
    }

    /// The parameters' locals: `_1..=_n`.
    pub fn args(&self) -> impl Iterator<Item = Local> {
        (1..=self.arg_count as u32).map(Local)
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Projection {
    /// `.n`: the nth field of a struct.
    Field(usize),
    /// `*`: what a reference points to.
    Deref,
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
    /// A struct value, with fields in declaration order.
    Struct {
        def: DefId,
        fields: Vec<Operand>,
    },
    Call {
        func: FuncId,
        args: Vec<Operand>,
    },
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

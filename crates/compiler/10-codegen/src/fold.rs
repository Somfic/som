//! Folding temporaries into where they're read, and the C expressions that makes.

use som_mir::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// Where a local is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Use {
    /// The whole right-hand side of an assignment.
    Assign,
    /// A call argument; C converts it to the parameter's type.
    Arg,
    /// A struct literal field; C converts it to the field's type.
    Field,
    Operand,
    Branch,
    Index,
    /// Anything else: borrowed, written through, or read through a projection.
    Other,
}

impl Use {
    /// Whether a temporary assigned `rvalue` can be folded into this use without changing
    /// what the program does.
    pub fn allows(self, rvalue: &Rvalue, narrow: bool) -> bool {
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
            Rvalue::Binary { op, .. } if narrow && op.is_arithmetic() => {
                matches!(self, Use::Assign | Use::Arg | Use::Field)
            }
            _ => self != Use::Other,
        }
    }
}

/// How often each local is assigned and read in a body, and where it was last read.
pub(crate) struct Usage {
    pub assigns: Vec<u32>,
    pub reads: Vec<u32>,
    pub read_as: Vec<Use>,
}

impl Usage {
    pub fn of(mir: &Mir, body: &Body) -> Usage {
        let n = body.locals.slots();
        let mut u = Usage {
            assigns: vec![0; n],
            reads: vec![0; n],
            read_as: vec![Use::Other; n],
        };
        for block in &body.blocks {
            for stmt in &block.stmts {
                match &stmt.kind {
                    StatementKind::Drop { place, flag, .. } => {
                        u.read(place.local, Use::Other);
                        place
                            .index_locals()
                            .for_each(|index| u.read(index, Use::Other));
                        if let Some(flag) = flag {
                            u.read(*flag, Use::Other);
                        }
                    }
                    StatementKind::Assign(dest, rvalue) => u.assign(mir, dest, rvalue),
                    _ => {}
                }
            }
            if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind {
                u.operand(condition, Use::Branch);
            }
        }
        u
    }

    fn assign(&mut self, mir: &Mir, dest: &Place, rvalue: &Rvalue) {
        if dest.projection.is_empty() {
            self.assigns[dest.local.id] += 1;
        } else {
            self.read(dest.local, Use::Other);
        }
        dest.index_locals()
            .for_each(|index| self.read(index, Use::Index));
        match rvalue {
            Rvalue::Use(op) => self.operand(op, Use::Assign),
            Rvalue::Ref { place, .. } => {
                self.read(place.local, Use::Other);
                place
                    .index_locals()
                    .for_each(|index| self.read(index, Use::Index));
            }
            Rvalue::Not(op) => self.operand(op, Use::Operand),
            // Fields of the compound literals they become.
            Rvalue::Some(op) => self.operand(op, Use::Field),
            Rvalue::Array { items, .. } => {
                items.iter().for_each(|item| self.operand(item, Use::Field))
            }
            Rvalue::Range { start, end, .. } => {
                self.operand(start, Use::Field);
                self.operand(end, Use::Field);
            }
            Rvalue::MakeSlice { start, len, .. } => {
                self.operand(start, Use::Field);
                self.operand(len, Use::Field);
            }
            Rvalue::Struct { fields, .. } => {
                fields.iter().for_each(|f| self.operand(f, Use::Field))
            }
            // Read once per element, by a loop.
            Rvalue::Repeat { operand, .. } => self.operand(operand, Use::Other),
            Rvalue::Cast { operand, .. } => self.operand(operand, Use::Operand),
            Rvalue::Binary { lhs, rhs, .. } => {
                self.operand(lhs, Use::Operand);
                self.operand(rhs, Use::Operand);
            }
            Rvalue::Intrinsic { args, .. } => {
                args.iter().for_each(|a| self.operand(a, Use::Operand))
            }
            Rvalue::Call { func, args } => {
                // A variadic function's extra arguments are only promoted, not converted.
                let fixed = mir.func(*func).params.len();
                for (i, a) in args.iter().enumerate() {
                    self.operand(a, if i < fixed { Use::Arg } else { Use::Operand });
                }
            }
        }
    }

    fn operand(&mut self, op: &Operand, as_: Use) {
        if let Some(place) = op.place() {
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
pub(crate) enum Prec {
    Other,
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Equality,
    Relational,
    Shift,
    Additive,
    Multiplicative,
    Atom,
}

impl Prec {
    pub fn of(op: BinOp) -> Prec {
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
            BinOp::BitAnd => Prec::BitAnd,
            BinOp::BitOr => Prec::BitOr,
            BinOp::BitXor => Prec::BitXor,
            BinOp::ShiftLeft | BinOp::ShiftRight => Prec::Shift,
        }
    }

    /// C ranks these below `==`, so they're always grouped explicitly.
    fn is_bitwise(self) -> bool {
        matches!(
            self,
            Prec::BitAnd | Prec::BitOr | Prec::BitXor | Prec::Shift
        )
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
pub(crate) struct Expr {
    pub text: String,
    pub prec: Prec,
}

impl Expr {
    pub fn atom(text: String) -> Expr {
        Expr {
            text,
            prec: Prec::Atom,
        }
    }

    pub fn compound(text: String) -> Expr {
        Expr {
            text,
            prec: Prec::Other,
        }
    }

    /// The expression as an operand of a unary operator or cast.
    pub fn operand(self) -> String {
        if self.prec == Prec::Atom {
            self.text
        } else {
            format!("({})", self.text)
        }
    }

    /// The expression as the left or right operand of `op`. Parentheses are left out only
    /// where C's precedence gives the same grouping and compilers don't warn about mixing:
    /// `a * b + c`, `a - b + c`, `a + b < c`, `a < b && c`.
    pub fn operand_of(self, op: BinOp, left: bool) -> String {
        let parent = Prec::of(op);
        let bare = match self.prec {
            Prec::Atom => true,
            // `a & b & c`; but `a << b << c` reads better grouped.
            child if child == parent => {
                left && (child.is_arithmetic()
                    || child.is_logical()
                    || matches!(child, Prec::BitAnd | Prec::BitOr | Prec::BitXor))
            }
            child if child.is_arithmetic() => {
                child > parent && !parent.is_logical() && !parent.is_bitwise()
            }
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
pub(crate) struct Pending {
    pub expr: Expr,
    /// The locals its value depends on: if one is assigned, it has to be written out first.
    pub reads: HashSet<Local>,
    /// Whether it reads memory something else could change: through a reference, or a local
    /// whose address is taken.
    pub memory: bool,
    /// Whether it has an effect, a call: then it can't move past anything.
    pub effect: bool,
}

/// The state while emitting one function.
pub(crate) struct FnState<'a> {
    pub body: &'a Body,
    pub locals: Vec<String>,
    pub usage: Usage,
    pub pending: RefCell<HashMap<Local, Pending>>,
    /// What the temporaries folded into the expression being built depend on: their reads,
    /// whether they read memory, and whether they have an effect.
    pub folded: RefCell<(HashSet<Local>, bool, bool)>,
    /// Locals whose address is taken, so a write through a reference can change them.
    pub address_taken: HashSet<Local>,
    /// How often the emitted code mentions each local, so only those mentioned get declared.
    pub mentioned: RefCell<HashMap<Local, u32>>,
    /// The line of code each local is first mentioned on, where it can be declared.
    pub first: RefCell<HashMap<Local, usize>>,
    /// The line of code being written.
    pub line: Cell<usize>,
}

impl FnState<'_> {
    pub fn mention(&self, local: Local) {
        *self.mentioned.borrow_mut().entry(local).or_default() += 1;
        self.first
            .borrow_mut()
            .entry(local)
            .or_insert(self.line.get());
    }

    /// Takes the pending temporary `local`, to fold it into the expression being built.
    pub fn fold(&self, local: Local) -> Option<Expr> {
        let p = self.pending.borrow_mut().remove(&local)?;
        let mut folded = self.folded.borrow_mut();
        folded.0.extend(p.reads);
        folded.1 |= p.memory;
        folded.2 |= p.effect;
        Some(p.expr)
    }
}

/// Whether `op` is a constant whose bytes are all zero, so `{0}` makes copies of it.
pub(crate) fn is_zero(op: &Operand) -> bool {
    match op {
        Operand::Const(c) => match &c.kind {
            ConstKind::Int(n) => *n == 0,
            ConstKind::Float(x) => *x == 0.0 && x.is_sign_positive(),
            ConstKind::Bool(b) => !b,
            ConstKind::Unit | ConstKind::Zeroed => true,
            ConstKind::Str(_)
            | ConstKind::Func(_)
            | ConstKind::Pending(_)
            | ConstKind::Aggregate(_) => false,
        },
        _ => false,
    }
}

/// The local a whole-local operand reads, if it is one.
pub(crate) fn operand_local(op: &Operand) -> Option<Local> {
    op.place()
        .filter(|place| place.projection.is_empty())
        .map(|place| place.local)
}

/// The locals `dest = rvalue` could fold pending temporaries into: whole operands, and
/// indexes.
pub(crate) fn statement_reads(dest: &Place, rvalue: &Rvalue) -> Vec<Local> {
    let mut read: Vec<Local> = dest.index_locals().collect();
    for place in rvalue.places() {
        if place.projection.is_empty() && !matches!(rvalue, Rvalue::Ref { .. }) {
            read.push(place.local);
        }
        read.extend(place.index_locals());
    }
    read
}

pub(crate) fn address_taken(body: &Body) -> HashSet<Local> {
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

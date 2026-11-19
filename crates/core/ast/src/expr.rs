use crate::{Id, Ident, Path, TypeExpr};

pub enum Expr {
    Hole,
    /// `f32` in `@if T == f32`. Only a primitive: a name, `T` or `Point`, is an
    /// [`Expr::Var`] the resolver finds a type for.
    Type(Id<TypeExpr>),
    /// `@fail(message)`: gets no value, like `Unreachable`.
    Fail(Id<Expr>),
    /// `@const f(x)`: a constant with no name ([`crate::Const::in_place`]).
    InPlace(Id<crate::Const>),
    Int(i128),
    Float(f64),
    Bool(bool),
    Str(Box<str>),
    None,
    Array(Vec<Id<Expr>>),
    Cast {
        value: Id<Expr>,
        ty: Id<TypeExpr>,
    },
    /// Either end can be left out where something else knows it: `s.slice(1..)`.
    Range {
        start: Option<Id<Expr>>,
        end: Option<Id<Expr>>,
    },
    ArrayRepeat {
        value: Id<Expr>,
        count: Id<TypeExpr>,
    },
    Var(Path),
    Binary {
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
    },
    Not(Id<Expr>),
    /// A literal's minus is part of it: `-1` is `Int(-1)`.
    Neg(Id<Expr>),
    BitNot(Id<Expr>),
    /// `@unreachable`: a value of whatever type is expected there, as it's never used.
    Unreachable,
    Borrow {
        mutable: bool,
        expr: Id<Expr>,
    },
    Deref(Id<Expr>),
    /// `out name`, as an argument: declares `name` in the surrounding block, zeroed, and
    /// passes `&mut name` for the function to fill in.
    Out(Ident),
    Assign {
        target: Id<Expr>,
        value: Id<Expr>,
    },
    Call {
        callee: Path,
        /// `Alloc<i32>::with_capacity(10)`: the arguments of the type whose function it is.
        owner_args: Vec<Id<TypeExpr>>,
        type_args: Vec<Id<TypeExpr>>,
        args: Vec<Id<Expr>>,
    },
    /// Calling a function's address: `ops[0](5)`, `(pick())(x)`. A name, `f(x)`, is a
    /// `Call`, even when it names a variable.
    CallValue {
        callee: Id<Expr>,
        args: Vec<Id<Expr>>,
    },
    MethodCall {
        receiver: Id<Expr>,
        method: Ident,
        args: Vec<Id<Expr>>,
    },
    Field {
        object: Id<Expr>,
        field: Ident,
    },
    Index {
        object: Id<Expr>,
        index: Id<Expr>,
    },
    Construct {
        ty: Path,
        /// Left out, the fields' values decide them.
        args: Vec<Id<TypeExpr>>,
        fields: Vec<(Ident, Id<Expr>)>,
    },
    Conditional {
        condition: Id<Expr>,
        then: Id<Expr>,
        otherwise: Id<Expr>,
    },
    Fallback {
        optional: Id<Expr>,
        fallback: Id<Expr>,
    },
    Match {
        scrutinee: Id<Expr>,
        arms: Vec<Arm>,
    },
    /// `"total: {n}"`: each value written through `Display`.
    Interpolated(Vec<Part>),
    Block {
        stmts: Vec<Id<Stmt>>,
        value: Option<Id<Expr>>,
    },
}

#[derive(Clone, Copy)]
pub enum Part {
    /// An `Expr::Str`.
    Text(Id<Expr>),
    Value(Id<Expr>),
}

pub struct Arm {
    pub pattern: Id<Pattern>,
    pub body: Id<Expr>,
}

pub enum Pattern {
    Wildcard,
    /// A bare name that's one of an enum's variants, like `Empty`, is that variant instead;
    /// the resolver tells them apart.
    Binding {
        name: Ident,
        mutable: bool,
    },
    Int(i128),
    Bool(bool),
    Str(Box<str>),
    /// Any other pattern matched against an optional matches the value it holds.
    None,
    Variant {
        path: Path,
        fields: PatternFields,
    },
}

pub enum PatternFields {
    Unit,
    Tuple(Vec<Id<Pattern>>),
    /// `rest` if `..` leaves the others out.
    Named {
        fields: Vec<(Ident, Id<Pattern>)>,
        rest: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Equals,
    NotEquals,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    /// Shifted by `a`'s width or more, no bits are left: `0`, or for `>>` of a negative
    /// number, `-1`.
    ShiftLeft,
    ShiftRight,
}

pub enum Stmt {
    Use(crate::Use),
    /// Its value can't name the function's locals.
    Const(Id<crate::Const>),
    Let {
        name: Ident,
        mutable: bool,
        ty: Option<Id<TypeExpr>>,
        value: Id<Expr>,
    },
    Expr(Id<Expr>),
    Loop {
        body: Vec<Id<Stmt>>,
    },
    While {
        condition: Id<Expr>,
        body: Vec<Id<Stmt>>,
    },
    /// `break x;` leaves the `for` loop whose variable is `x`, and every loop inside it.
    Break(Option<Ident>),
    Return(Option<Id<Expr>>),
    /// `continue x;` goes on with the next `x` of the `for` loop whose variable it is.
    Continue(Option<Ident>),
    For {
        name: Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: Vec<Id<Stmt>>,
    },
    /// The condition is a `bool`, or an optional: `if x` is `if x != none`.
    If {
        condition: Id<Expr>,
        then: Vec<Id<Stmt>>,
        otherwise: Option<Vec<Id<Stmt>>>,
    },
}

use crate::{Id, Ident, Path, TypeExpr};

pub enum Expr {
    /// Placeholder for code that failed to parse.
    Hole,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),
    /// `none`: an optional without a value.
    None,
    /// `[a, b, c]`
    Array(Vec<Id<Expr>>),
    /// `value as ty`: a number converted to another number type.
    Cast {
        value: Id<Expr>,
        ty: Id<TypeExpr>,
    },
    /// `start..end`: the integers from `start` up to, not including, `end`. Either end can be
    /// left out where something else knows it: `s.slice(1..)`.
    Range {
        start: Option<Id<Expr>>,
        end: Option<Id<Expr>>,
    },
    /// `[value; count]`: `count` copies of `value`.
    ArrayRepeat {
        value: Id<Expr>,
        /// How many: a number, or a value parameter. Written like an array type's length.
        count: Id<TypeExpr>,
    },
    Var(Path),
    Binary {
        op: BinOp,
        lhs: Id<Expr>,
        rhs: Id<Expr>,
    },
    /// `!expr`
    Not(Id<Expr>),
    /// `-expr`. A literal's minus is part of it: `-1` is `Int(-1)`.
    Neg(Id<Expr>),
    /// `&expr` or `&mut expr`
    Borrow {
        mutable: bool,
        expr: Id<Expr>,
    },
    /// `*expr`
    Deref(Id<Expr>),
    /// `out name`, as an argument: declares `name` in the surrounding block, starting
    /// out zeroed, and passes `&mut name` for the function to fill in.
    Out(Ident),
    /// `target = value`
    Assign {
        target: Id<Expr>,
        value: Id<Expr>,
    },
    /// `math::add(1, 2)`
    Call {
        callee: Path,
        args: Vec<Id<Expr>>,
    },
    /// `point.sum(1)`
    MethodCall {
        receiver: Id<Expr>,
        method: Ident,
        args: Vec<Id<Expr>>,
    },
    /// `point.x`
    Field {
        object: Id<Expr>,
        field: Ident,
    },
    /// `Point { x: 1, y: 2 }`, or `Pair<i32> { a: 1, b: 2 }`
    Construct {
        ty: Path,
        /// A generic struct's type arguments, if they're written: `i32` in `Pair<i32> { … }`.
        /// Left out, the fields' values decide them.
        args: Vec<Id<TypeExpr>>,
        fields: Vec<(Ident, Id<Expr>)>,
    },
    /// `then if condition else otherwise`
    Conditional {
        condition: Id<Expr>,
        then: Id<Expr>,
        otherwise: Id<Expr>,
    },
    /// `optional else fallback`: the optional's value, or `fallback` if it has none.
    Fallback {
        optional: Id<Expr>,
        fallback: Id<Expr>,
    },
    /// `match scrutinee { pattern => value, … }`: the value of the first arm whose pattern
    /// matches.
    Match {
        scrutinee: Id<Expr>,
        arms: Vec<Arm>,
    },
    /// `{ stmts; value }`
    Block {
        stmts: Vec<Id<Stmt>>,
        value: Option<Id<Expr>>,
    },
}

/// `pattern => body` in a `match`.
pub struct Arm {
    pub pattern: Id<Pattern>,
    pub body: Id<Expr>,
}

/// What a `match` arm compares a value against.
pub enum Pattern {
    /// `_`: anything, ignored.
    Wildcard,
    /// `name` or `mut name`: anything, bound to `name`. A bare name that's one of an enum's
    /// variants, like `Empty`, is that variant instead; the resolver tells them apart.
    Binding { name: Ident, mutable: bool },
    /// `3`, `-1`
    Int(i64),
    /// `true` or `false`
    Bool(bool),
    /// `none`: an optional without a value. Any other pattern matched against an optional
    /// matches the value it holds.
    None,
    /// `Circle(r)`, `Rect { w, h }`, `Shape::Empty`: one of an enum's variants, and patterns
    /// for what it holds.
    Variant { path: Path, fields: PatternFields },
}

pub enum PatternFields {
    /// `Shape::Empty`
    Unit,
    /// `Circle(r)`: a pattern for each field, in order.
    Tuple(Vec<Id<Pattern>>),
    /// `Rect { w, h: height, .. }`: patterns for fields by name; `rest` if `..` leaves the
    /// others out. `w` alone is short for `w: w`.
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
}

pub enum Stmt {
    /// `use som::data;` inside a block: what the module has is in scope until the block ends.
    Use(crate::Use),
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
    /// `break;`: leaves the innermost `loop`, `while` or `for`. `break x;` leaves the `for`
    /// loop whose variable is `x`, and every loop inside it.
    Break(Option<Ident>),
    /// `return value;` or `return;`: leaves the function, with `value` as its result.
    Return(Option<Id<Expr>>),
    /// `continue;`: goes on with the innermost loop's next time round. `continue x;` goes on
    /// with the next `x` of the `for` loop whose variable it is.
    Continue(Option<Ident>),
    /// `for name in iterable { body }`: runs `body` once for each integer in a range, or each
    /// element of an array or slice, with `name` bound to it.
    For {
        name: Ident,
        mutable: bool,
        iterable: Id<Expr>,
        body: Vec<Id<Stmt>>,
    },
    /// `if condition { then } else { otherwise }`. The condition is a `bool`, or an optional
    /// that's true when it holds a value: `if x` is `if x != none`.
    If {
        condition: Id<Expr>,
        then: Vec<Id<Stmt>>,
        otherwise: Option<Vec<Id<Stmt>>>,
    },
}

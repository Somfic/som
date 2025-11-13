use crate::{
    ast::{Binary, Expr, Expression, Group, Primary, Unary},
    Phase,
};

pub trait Pseudo {
    fn pseudo(&self) -> String;
}

impl<P: Phase> Pseudo for Expression<P> {
    fn pseudo(&self) -> String {
        self.expr.pseudo()
    }
}

impl<P: Phase> Pseudo for Expr<P> {
    fn pseudo(&self) -> String {
        match self {
            Expr::Primary(p) => p.pseudo(),
            Expr::Unary(u) => u.pseudo(),
            Expr::Binary(b) => b.pseudo(),
            Expr::Group(g) => g.pseudo(),
        }
    }
}

impl Pseudo for Primary {
    fn pseudo(&self) -> String {
        match self {
            Primary::Boolean(b) => b.to_string(),
            Primary::I32(i) => i.to_string(),
            Primary::I64(i) => format!("{}i64", i),
            Primary::Decimal(d) => d.to_string(),
            Primary::String(s) => format!("\"{}\"", s),
            Primary::Character(c) => format!("'{}'", c),
            Primary::Identifier(id) => id.name.to_string(),
        }
    }
}

impl<P: Phase> Pseudo for Unary<P> {
    fn pseudo(&self) -> String {
        match self {
            Unary::Negate(expr) => format!("(-{})", expr.pseudo()),
        }
    }
}

impl<P: Phase> Pseudo for Binary<P> {
    fn pseudo(&self) -> String {
        match self {
            Binary::Add(lhs, rhs) => format!("({} + {})", lhs.pseudo(), rhs.pseudo()),
            Binary::Subtract(lhs, rhs) => format!("({} - {})", lhs.pseudo(), rhs.pseudo()),
            Binary::Multiply(lhs, rhs) => format!("({} * {})", lhs.pseudo(), rhs.pseudo()),
            Binary::Divide(lhs, rhs) => format!("({} / {})", lhs.pseudo(), rhs.pseudo()),
        }
    }
}

impl<P: Phase> Pseudo for Group<P> {
    fn pseudo(&self) -> String {
        format!("({})", self.expr.pseudo())
    }
}

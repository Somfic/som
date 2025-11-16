use crate::{
    ast::{
        Binary, BinaryOperation, Block, Expr, Expression, Group, Primary, Pseudo, Ternary, Unary,
    },
    Phase,
};

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
            Expr::Block(b) => b.pseudo(),
            Expr::Ternary(t) => t.pseudo(),
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
        let lhs = self.lhs.pseudo();
        let rhs = self.rhs.pseudo();

        match self.op {
            BinaryOperation::Add => format!("({} + {})", lhs, rhs),
            BinaryOperation::Subtract => format!("({} - {})", lhs, rhs),
            BinaryOperation::Multiply => format!("({} * {})", lhs, rhs),
            BinaryOperation::Divide => format!("({} / {})", lhs, rhs),
        }
    }
}

impl<P: Phase> Pseudo for Group<P> {
    fn pseudo(&self) -> String {
        format!("({})", self.expr.pseudo())
    }
}

impl<P: Phase> Pseudo for Block<P> {
    fn pseudo(&self) -> String {
        todo!()
    }
}

impl<P: Phase> Pseudo for Ternary<P> {
    fn pseudo(&self) -> String {
        format!(
            "({} ? {} : {})",
            self.condition.pseudo(),
            self.truthy.pseudo(),
            self.falsy.pseudo()
        )
    }
}

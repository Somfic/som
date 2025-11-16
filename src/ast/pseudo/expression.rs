use crate::{
    ast::{
        Binary, BinaryOperation, Block, Expression, Group, Primary, PrimaryKind, Pseudo, Ternary,
        Unary, UnaryOperation,
    },
    Phase,
};

impl<P: Phase> Pseudo for Expression<P> {
    fn pseudo(&self) -> String {
        match self {
            Expression::Primary(p) => p.pseudo(),
            Expression::Unary(u) => u.pseudo(),
            Expression::Binary(b) => b.pseudo(),
            Expression::Group(g) => g.pseudo(),
            Expression::Block(b) => b.pseudo(),
            Expression::Ternary(t) => t.pseudo(),
        }
    }
}

impl<P: Phase> Pseudo for Primary<P> {
    fn pseudo(&self) -> String {
        match &self.kind {
            PrimaryKind::Boolean(b) => b.to_string(),
            PrimaryKind::I32(i) => i.to_string(),
            PrimaryKind::I64(i) => format!("{}i64", i),
            PrimaryKind::Decimal(d) => d.to_string(),
            PrimaryKind::String(s) => format!("\"{}\"", s),
            PrimaryKind::Character(c) => format!("'{}'", c),
            PrimaryKind::Identifier(id) => id.name.to_string(),
        }
    }
}

impl<P: Phase> Pseudo for Unary<P> {
    fn pseudo(&self) -> String {
        match &self.op {
            UnaryOperation::Negate => format!("(-{})", self.value.pseudo()),
        }
    }
}

impl<P: Phase> Pseudo for Binary<P> {
    fn pseudo(&self) -> String {
        let lhs = self.lhs.pseudo();
        let rhs = self.rhs.pseudo();

        match self.op {
            BinaryOperation::Add => format!("{} + {}", lhs, rhs),
            BinaryOperation::Subtract => format!("{} - {}", lhs, rhs),
            BinaryOperation::Multiply => format!("{} * {}", lhs, rhs),
            BinaryOperation::Divide => format!("{} / {}", lhs, rhs),
            BinaryOperation::LessThan => format!("{} < {}", lhs, rhs),
            BinaryOperation::LessThanOrEqual => format!("{} <= {}", lhs, rhs),
            BinaryOperation::GreaterThan => format!("{} > {}", lhs, rhs),
            BinaryOperation::GreaterThanOrEqual => format!("{} >= {}", lhs, rhs),
            BinaryOperation::Equality => format!("{} == {}", lhs, rhs),
            BinaryOperation::Inequality => format!("{} != {}", lhs, rhs),
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

//! Going through a body in the order it runs, learning what holds as it goes.

use som_ast::{BinOp, Expr, Id, Stmt};
use som_base::{Fix, Message, Span};
use som_resolve::DefId;
use som_typeck::Ty;

use crate::facts::{Compare, Cond, Value};
use crate::{Place, Prover, Within, as_is};

impl Prover<'_> {
    /// Statements, in the scope of whoever goes through them: what an early `return` in
    /// them teaches lasts as long as that scope.
    fn stmts(&mut self, stmts: &[Id<Stmt>], tail: bool) {
        for (i, &stmt) in stmts.iter().enumerate() {
            self.within.push(Within {
                stmt,
                rest: stmts[i + 1..].to_vec(),
                tail,
            });
            self.stmt(stmt);
            self.within.pop();
            // `if i >= xs.len { return; }`: the rest only runs where it's false.
            if let Stmt::If {
                condition,
                then,
                otherwise,
            } = &self.ast.stmts[stmt]
            {
                let then_leaves = self.flow().leaves(then);
                let otherwise_leaves = otherwise.as_ref().is_some_and(|o| self.flow().leaves(o));
                if then_leaves != otherwise_leaves {
                    let cond = self.cond(*condition, otherwise_leaves, &as_is);
                    self.facts.assume(&cond);
                }
            }
        }
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            // A constant's value isn't proven here, wherever it's written.
            Stmt::Use(_) | Stmt::Const(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Let { value, .. } => {
                self.expr(*value);
                if let Some(def) = self.names.local(id) {
                    self.learn_let(def, *value);
                }
            }
            // `@for f in T` over a generic type, proving one instance: its copy.
            Stmt::Expr(_) if let Some(arm) = self.typed.chosen_arm(id) => self.expr(arm),
            Stmt::Expr(expr) => self.expr(*expr),
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.expr(*value);
                }
            }
            // An `@if` checked for each instance of its function: proven there, not here.
            Stmt::If { .. } if self.typed.is_region(id) => {}
            // `@if`: only the branch it picks is there, and nothing's checked of the condition
            // as the program runs.
            Stmt::If {
                then, otherwise, ..
            } if let Some(holds) = self.typed.settled_if(id) => {
                let branch = if holds {
                    Some(then)
                } else {
                    otherwise.as_ref()
                };
                if let Some(branch) = branch {
                    self.stmts(branch, false);
                }
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                self.expr(*condition);
                self.branch(*condition, true, |p| p.stmts(then, false));
                if let Some(otherwise) = otherwise {
                    self.branch(*condition, false, |p| p.stmts(otherwise, false));
                }
            }
            Stmt::Loop { body } => {
                self.forget_changed_in(body);
                let mark = self.facts.mark();
                self.stmts(body, false);
                self.facts.forget_since(mark);
            }
            Stmt::While { condition, body } => {
                self.forget_changed_in(body);
                self.expr(*condition);
                self.branch(*condition, true, |p| p.stmts(body, false));
                // `while i < n { … }` ends with `i >= n`, unless a `break` leaves it sooner.
                if !self.flow().breaks(body) {
                    let cond = self.cond(*condition, false, &as_is);
                    self.facts.assume(&cond);
                }
            }
            Stmt::For { iterable, body, .. } => {
                self.forget_changed_in(body);
                self.expr(*iterable);
                let mark = self.facts.mark();
                self.assume_range(id, *iterable);
                self.stmts(body, false);
                self.facts.forget_since(mark);
            }
        }
    }

    /// What local `def` is from its `let`, until it changes: `let last = xs.len - 1`, `last`
    /// is that; `let p = Point { x: 1, y: 2 }`, `p.x` is 1; `let s: &[i32] = [1, 2, 3]`,
    /// `s.len` is 3.
    fn learn_let(&mut self, def: DefId, value: Id<Expr>) {
        if self.is_integer(value)
            && let Some((known, needs)) = self.value_of(value, &as_is)
        {
            let local = Value::place(Place::var(def));
            if self.is_unsigned_ty(self.typed.local(def)) {
                self.facts.unsigned(local.term_place().clone());
            }
            if self.all_hold(&needs) {
                self.facts
                    .assume(&Cond::compare(&local, Compare::Equal, &known));
            }
        }
        self.learn_fields(def, Vec::new(), value);
        if let Expr::Array(values) = &self.ast.exprs[value]
            && matches!(self.typed.ty(self.typed.local(def)), Ty::Ref { to, .. }
                if matches!(self.typed.ty(*to), Ty::Slice(_)))
        {
            let len = Place::Path {
                root: def,
                path: vec![som_lang::LEN.into()],
            };
            self.facts.unsigned(len.clone());
            let count = Value::number(values.len() as i128);
            self.facts
                .assume(&Cond::compare(&Value::place(len), Compare::Equal, &count));
        }
    }

    /// Runs `f` where `condition` is `truth`.
    fn branch(&mut self, condition: Id<Expr>, truth: bool, f: impl FnOnce(&mut Self)) {
        let mark = self.facts.mark();
        let cond = self.cond(condition, truth, &as_is);
        self.facts.assume(&cond);
        f(self);
        self.facts.forget_since(mark);
    }

    /// What the integer fields of a struct written out, `value`, put at `path` in local
    /// `root` are. Fields of fields too.
    fn learn_fields(&mut self, root: DefId, path: Vec<Box<str>>, value: Id<Expr>) {
        let Expr::Construct { fields, .. } = &self.ast.exprs[value] else {
            return;
        };
        for (name, field) in fields {
            let mut inner = path.clone();
            inner.push(name.name.clone());
            if self.is_integer(*field)
                && let Some(known) = self.known_value(*field)
            {
                let place = Value::place(Place::Path {
                    root,
                    path: inner.clone(),
                });
                if self.is_unsigned(*field) {
                    self.facts.unsigned(place.term_place().clone());
                }
                self.facts
                    .assume(&Cond::compare(&place, Compare::Equal, &known));
            }
            self.learn_fields(root, inner, *field);
        }
    }

    /// `for i in a..b`: `a <= i` and `i < b`, each time round. Not of a `mut` variable,
    /// which the body can change.
    fn assume_range(&mut self, stmt: Id<Stmt>, iterable: Id<Expr>) {
        let Stmt::For { mutable: false, .. } = self.ast.stmts[stmt] else {
            return;
        };
        let Expr::Range { start, end } = self.ast.exprs[iterable] else {
            return;
        };
        let Some(def) = self.names.local(stmt) else {
            return;
        };
        let var = Value::place(Place::var(def));
        if self.is_unsigned_ty(self.typed.local(def)) {
            self.facts.unsigned(var.term_place().clone());
        }
        let start = match start {
            Some(start) => self.known_value(start),
            None => Some(Value::number(0)),
        };
        if let Some(start) = start {
            self.facts
                .assume(&Cond::compare(&start, Compare::LessOrEqual, &var));
        }
        if let Some(end) = end
            && let Some(end) = self.known_value(end)
        {
            self.facts.assume(&Cond::compare(&var, Compare::Less, &end));
        }
    }

    pub(crate) fn expr(&mut self, id: Id<Expr>) {
        match &self.ast.exprs[id] {
            Expr::Fail(message) | Expr::Raise(message) | Expr::Propagate(message) => {
                self.expr(*message)
            }
            // Only when it failed.
            Expr::Recover { failable, body, .. } => {
                self.expr(*failable);
                let mark = self.facts.mark();
                self.expr(*body);
                self.facts.forget_since(mark);
            }
            // A constant's value isn't proven here, wherever it's written.
            Expr::InPlace(_)
            | Expr::Hole
            | Expr::Type(_)
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None
            | Expr::Var(_)
            | Expr::Unreachable
            | Expr::Out(_) => {}
            Expr::Array(values) => values.iter().for_each(|&v| self.expr(v)),
            Expr::Interpolated(parts) => {
                for &part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.expr(value);
                    }
                }
            }
            Expr::Range { start, end } => {
                start.iter().chain(end).for_each(|&e| self.expr(e));
            }
            Expr::Cast { value: inner, .. }
            | Expr::ArrayRepeat { value: inner, .. }
            | Expr::Not(inner)
            | Expr::Neg(inner)
            | Expr::BitNot(inner)
            | Expr::Deref(inner) => self.expr(*inner),
            Expr::Binary { op, lhs, rhs } => self.binary(*op, *lhs, *rhs),
            Expr::Borrow { mutable, expr } => {
                self.expr(*expr);
                if *mutable {
                    self.changed(*expr);
                }
            }
            Expr::Assign { target, value } => self.assign(*target, *value),
            Expr::Call { args, .. } => {
                args.iter().for_each(|&a| self.expr(a));
                if let Some(func) = self.callee(id) {
                    self.check_call(id, func, 0, None, args);
                }
            }
            // Its body runs later, called with values nothing's known of. What's known around it
            // still holds there: it only reads what it captures, copied or lent.
            Expr::Closure { body, .. } => {
                let around = self.facts.clone();
                self.expr(*body);
                self.facts = around;
            }
            // Nothing's known of the function: `&mut` arguments are changed where they're lent.
            Expr::CallValue { callee, args } => {
                self.expr(*callee);
                args.iter().for_each(|&a| self.expr(a));
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.expr(*receiver);
                args.iter().for_each(|&a| self.expr(a));
                if let Some(func) = self.method(id) {
                    self.check_call(id, func, 1, Some(*receiver), args);
                    if self.takes_mut_self(func) {
                        self.changed(*receiver);
                    }
                }
            }
            Expr::Field { object, .. } => {
                self.expr(*object);
                // A property's `@where`.
                if let Some(func) = self.method(id) {
                    self.check_call(id, func, 1, Some(*object), &[]);
                }
            }
            Expr::Index { object, index } => {
                self.expr(*object);
                self.expr(*index);
                self.check_index(id, *object, *index, true);
            }
            Expr::Construct { fields, .. } => fields.iter().for_each(|&(_, v)| self.expr(v)),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.expr(*condition);
                self.branch(*condition, true, |p| p.expr(*then));
                self.branch(*condition, false, |p| p.expr(*otherwise));
            }
            Expr::Fallback { optional, fallback } => {
                self.expr(*optional);
                self.expr(*fallback);
            }
            Expr::Match { .. } if let Some(copy) = self.typed.chosen_match(id) => self.expr(copy),
            Expr::Match { scrutinee, arms } => {
                self.expr(*scrutinee);
                for arm in arms {
                    let mark = self.facts.mark();
                    self.expr(arm.body);
                    self.facts.forget_since(mark);
                }
            }
            Expr::Block { stmts, value } => {
                let mark = self.facts.mark();
                self.stmts(stmts, value.is_some());
                if let Some(value) = value {
                    self.expr(*value);
                }
                self.facts.forget_since(mark);
            }
        }
    }

    fn binary(&mut self, op: BinOp, lhs: Id<Expr>, rhs: Id<Expr>) {
        self.expr(lhs);
        // `i < xs.len and xs[i] == 0`: the right side only runs where the left is true, or
        // for `||`, false.
        match op {
            BinOp::And => self.branch(lhs, true, |p| p.expr(rhs)),
            BinOp::Or => self.branch(lhs, false, |p| p.expr(rhs)),
            _ => self.expr(rhs),
        }
        if matches!(op, BinOp::Divide | BinOp::Modulo) && self.is_integer(lhs) {
            self.check_divisor(rhs);
        }
    }

    fn assign(&mut self, target: Id<Expr>, value: Id<Expr>) {
        self.expr(value);
        self.target(target);
        self.changed(target);
        // `i = n`: `i` is `n` from here, if `n` isn't `i` itself.
        if let Some(def) = self.local_var(target)
            && self.is_integer(value)
            && !self.mentions(value, def)
            && let Some(known) = self.known_value(value)
        {
            let var = Value::place(Place::var(def));
            self.facts
                .assume(&Cond::compare(&var, Compare::Equal, &known));
        }
    }

    /// An assignment's target: what's evaluated to find where it goes, `i` in `xs[i] = v`.
    fn target(&mut self, target: Id<Expr>) {
        match &self.ast.exprs[target] {
            Expr::Var(_) => {}
            Expr::Field { object, .. } | Expr::Deref(object) => self.target(*object),
            Expr::Index { object, index } => {
                self.target(*object);
                self.expr(*index);
                self.check_index(target, *object, *index, false);
            }
            _ => self.expr(target),
        }
    }

    /// A fix putting the statement being gone through inside `if check { … }`: with the
    /// rest of its block, for a `let`, whose variable that uses. Not for a `return`, after
    /// which the function would need another, nor a `let` in a block ending in a value.
    pub(crate) fn check_fix(&self, check: &str, title: Message) -> Option<Fix> {
        let within = self.within.last()?;
        let last = match self.ast.stmts[within.stmt] {
            Stmt::Expr(_) => within.stmt,
            Stmt::Let { .. } if !within.tail => *within.rest.last().unwrap_or(&within.stmt),
            _ => return None,
        };
        let first = self.ast.stmts.span(within.stmt);
        let end = self.ast.stmts.span(last);
        let mut end_offset = end.start_offset + end.length;
        // With the last one's `;`, which goes inside.
        if first.source.content()[end_offset..].starts_with(';') {
            end_offset += 1;
        }
        let span = Span::from_range(first.start_offset..end_offset, first.source.clone());
        let indent = " ".repeat(span.start.col.saturating_sub(1));
        let text = span.get_text();
        let body: Vec<String> = (text.lines().enumerate())
            .map(|(i, line)| match i {
                0 => format!("{indent}    {line}"),
                _ if line.is_empty() => String::new(),
                _ => format!("    {line}"),
            })
            .collect();
        let wrapped = format!("if {check} {{\n{}\n{indent}}}", body.join("\n"));
        Some(Fix::replace(title, span, wrapped))
    }
}

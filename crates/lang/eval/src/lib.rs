//! The walker: a tree-walking evaluator that interprets typed HIR by driving
//! the reactive runtime. Pure logic runs inline; `let` bindings become signals,
//! layout becomes renderer nodes, and handlers/interpolations become closures
//! that re-walk their HIR subtree when their signals change.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use som_canvas::{Attribute, Event, Node, Tag};
use som_common::{GenId, Id};
use som_hir::{BinaryOp, Binding, Expr, Hir, Layout, Root, Stmt, TextPart, UnaryOp};
use som_runtime::{
    Signal, bind_text, create_element, create_text, derived, insert, on, set_attr, signal,
};

/// A runtime value. Every `let` binding holds one inside a `Signal`.
#[derive(Clone, Debug)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
    Nothing,
}

impl Value {
    fn int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            other => panic!("expected an int, found {other:?}"),
        }
    }

    fn bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            other => panic!("expected a bool, found {other:?}"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Str(s) => f.write_str(s),
            Value::Nothing => Ok(()),
        }
    }
}

type Env = Rc<RefCell<HashMap<Id<Binding>, Signal<Value>>>>;

/// Shared, cheaply-cloneable evaluation context. Closures stored in the runtime
/// (event handlers, text bindings) capture a clone so they can re-walk later.
#[derive(Clone)]
struct Ctx {
    hir: Rc<Hir>,
    env: Env,
}

/// Build a program's UI under `body`, wiring reactivity through the runtime.
/// Must run with a renderer installed (and, for Blitz, active).
pub fn build_ui(hir: Rc<Hir>, body: GenId<Node>) {
    let ctx = Ctx {
        hir,
        env: Rc::new(RefCell::new(HashMap::new())),
    };

    for i in 0..ctx.hir.root.len() {
        match &ctx.hir.root[i] {
            Root::Stmt(stmt) => ctx.eval_stmt(*stmt),
            Root::Layout(layout) => {
                let node = ctx.build_layout(*layout);
                insert(body, node, None);
            }
        }
    }
}

impl Ctx {
    fn eval_stmt(&self, id: Id<Stmt>) {
        match self.hir.get_stmt(id) {
            Stmt::Let { binding, expr, .. } => {
                // Mutable bindings (ever assigned) are state; the rest are
                // derived — their initialiser re-runs when its signals change.
                let sig = if self.hir.binding(*binding).mutable {
                    signal(self.eval(*expr))
                } else {
                    let ctx = self.clone();
                    let expr = *expr;
                    derived(move || ctx.eval(expr))
                };
                self.env.borrow_mut().insert(*binding, sig);
            }
            Stmt::Expr { expr, .. } => {
                self.eval(*expr);
            }
            Stmt::Error { .. } => {}
        }
    }

    fn eval(&self, id: Id<Expr>) -> Value {
        match self.hir.get_expr(id) {
            Expr::Int { value, .. } => Value::Int(*value),
            Expr::Bool { value, .. } => Value::Bool(*value),
            Expr::Str { value, .. } => Value::Str(value.to_string()),
            Expr::Error { .. } => Value::Nothing,

            Expr::Variable { binding, .. } => match binding {
                Some(b) => self
                    .env
                    .borrow()
                    .get(b)
                    .cloned()
                    .expect("bound variable has a signal")
                    .get(),
                None => Value::Nothing,
            },

            Expr::Assignment { binding, value, .. } => {
                let v = self.eval(*value);
                if let Some(b) = binding {
                    let sig = self.env.borrow().get(b).cloned();
                    if let Some(sig) = sig {
                        sig.set(v);
                    }
                }
                Value::Nothing
            }

            Expr::Unary { op, operand, .. } => {
                let v = self.eval(*operand);
                match op {
                    UnaryOp::Negate => Value::Int(-v.int()),
                    UnaryOp::Not => Value::Bool(!v.bool()),
                }
            }

            Expr::Binary { lhs, op, rhs, .. } => {
                let l = self.eval(*lhs);
                let r = self.eval(*rhs);
                match op {
                    BinaryOp::Add => Value::Int(l.int() + r.int()),
                    BinaryOp::Subtract => Value::Int(l.int() - r.int()),
                    BinaryOp::Multiply => Value::Int(l.int() * r.int()),
                    BinaryOp::Divide => Value::Int(l.int() / r.int()),
                    BinaryOp::LessThan => Value::Bool(l.int() < r.int()),
                    BinaryOp::LessThanOrEquals => Value::Bool(l.int() <= r.int()),
                    BinaryOp::GreaterThan => Value::Bool(l.int() > r.int()),
                    BinaryOp::GreaterThanOrEquals => Value::Bool(l.int() >= r.int()),
                    BinaryOp::And => Value::Bool(l.bool() && r.bool()),
                    BinaryOp::Or => Value::Bool(l.bool() || r.bool()),
                    BinaryOp::Equals => Value::Bool(value_eq(&l, &r)),
                    BinaryOp::NotEquals => Value::Bool(!value_eq(&l, &r)),
                }
            }

            Expr::Condition {
                condition,
                truthy,
                falsy,
                ..
            } => {
                if self.eval(*condition).bool() {
                    self.eval(*truthy)
                } else {
                    self.eval(*falsy)
                }
            }

            Expr::Block { stmts, value, .. } => {
                let stmts = stmts.clone();
                let value = *value;
                for stmt in stmts {
                    self.eval_stmt(stmt);
                }
                match value {
                    Some(expr) => self.eval(expr),
                    None => Value::Nothing,
                }
            }
        }
    }

    fn build_layout(&self, id: Id<Layout>) -> GenId<Node> {
        match self.hir.get_layout(id) {
            Layout::Element {
                tag,
                events,
                attr,
                children,
                ..
            } => {
                let node = create_element(tag_of(tag));

                for (name, &value) in attr {
                    if let Some(attribute) = attr_of(name) {
                        let text = self.eval(value).to_string();
                        set_attr(node, attribute, &text);
                    }
                }

                for (name, &body) in events {
                    if let Some(event) = event_of(name) {
                        let ctx = self.clone();
                        on(node, event, move || {
                            ctx.eval(body);
                        });
                    }
                }

                for &child in children {
                    let child_node = self.build_layout(child);
                    insert(node, child_node, None);
                }

                node
            }

            Layout::Text { .. } => {
                let node = create_text();
                let ctx = self.clone();
                bind_text(node, move || ctx.text_of(id));
                node
            }
        }
    }

    /// Render a text node's parts, reading (and subscribing to) any signals
    /// referenced by its interpolations.
    fn text_of(&self, id: Id<Layout>) -> String {
        let Layout::Text { text, .. } = self.hir.get_layout(id) else {
            return String::new();
        };

        let mut out = String::new();
        for part in text {
            match part {
                TextPart::Str { text, .. } => out.push_str(text),
                TextPart::Interp { value, .. } => {
                    out.push_str(&self.eval(*value).to_string());
                }
            }
        }
        out
    }
}

fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        _ => false,
    }
}

fn tag_of(name: &str) -> Tag {
    match name {
        "main" => Tag::Main,
        "button" => Tag::Button,
        _ => Tag::Block,
    }
}

fn event_of(name: &str) -> Option<Event> {
    match name {
        "click" => Some(Event::Click),
        _ => None,
    }
}

fn attr_of(name: &str) -> Option<Attribute> {
    match name {
        "style" => Some(Attribute::Style),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use som_canvas::Tag;
    use som_canvas_mock::MockRenderer;
    use som_common::{DiagnosticSink, Id};
    use som_runtime::{create_element, install, take};

    use super::build_ui;

    fn hir_of(src: &str) -> Rc<som_hir::Hir> {
        let mut diags = DiagnosticSink::new();
        let ast = som_ast::parse(Id::new(0), src, &mut diags);
        let (hir, _tcx) = som_hir::typeck(&ast, &mut diags);
        assert!(!diags.has_errors(), "unexpected diagnostics");
        Rc::new(hir)
    }

    #[test]
    fn string_binding_interpolates() {
        install(Box::new(MockRenderer::new()));
        let body = create_element(Tag::Block);

        let hir = hir_of("let name = \"world\"\nmain\n  \"hello {name}\"\n");
        build_ui(hir, body);

        let renderer = take().unwrap();
        let any: &dyn std::any::Any = &*renderer;
        let mock = any.downcast_ref::<MockRenderer>().unwrap();

        assert!(
            mock.log()
                .iter()
                .any(|line| line == r#"set_text(text#2, "hello world")"#),
            "expected the interpolated string, got: {:?}",
            mock.log()
        );
    }

    #[test]
    fn derived_binding_computes() {
        install(Box::new(MockRenderer::new()));
        let body = create_element(Tag::Block);

        // `count` and `doubled` are never assigned → both are derived; the text
        // node should render the chained computed value.
        let hir = hir_of("let count = 5\nlet doubled = count * 2\nmain\n  \"{doubled}\"\n");
        build_ui(hir, body);

        let renderer = take().unwrap();
        let any: &dyn std::any::Any = &*renderer;
        let mock = any.downcast_ref::<MockRenderer>().unwrap();

        assert!(
            mock.log()
                .iter()
                .any(|line| line == r#"set_text(text#2, "10")"#),
            "expected the derived value, got: {:?}",
            mock.log()
        );
    }

    #[test]
    fn builds_the_counter_render_calls() {
        install(Box::new(MockRenderer::new()));
        let body = create_element(Tag::Block);

        let hir = hir_of(
            "let count = 0\nmain\n  button @click: count += 1\n    \"+1\"\n  \"count: {count}\"\n",
        );
        build_ui(hir, body);

        let renderer = take().unwrap();
        let any: &dyn std::any::Any = &*renderer;
        let mock = any.downcast_ref::<MockRenderer>().unwrap();

        assert_eq!(
            mock.log(),
            &[
                r#"create_element("block") -> #0"#,
                r#"create_element("main") -> #1"#,
                r#"create_element("button") -> #2"#,
                r#"listen(button#2, "click", handler=#0)"#,
                "create_text() -> #3",
                r#"set_text(text#3, "+1")"#,
                "insert(parent=button#2, child=text#3, before=None)",
                "insert(parent=main#1, child=button#2, before=None)",
                "create_text() -> #4",
                r#"set_text(text#4, "count: 0")"#,
                "insert(parent=main#1, child=text#4, before=None)",
                "insert(parent=block#0, child=main#1, before=None)",
            ]
        );
    }
}

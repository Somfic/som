//! A stable text form of the syntax tree, for `som ... --emit ast`.
//!
//! One node per line, children indented by two spaces. There are no spans or
//! file paths, so dumps diff cleanly between machines and between compiler
//! implementations.
//!
//! ```text
//! module ""
//!   fn add(a: i32, b: i32) -> i32
//!     block
//!       value
//!         binary +
//!           var a
//!           var b
//! ```

use std::fmt::Write;

use crate::*;

impl Ast {
    pub fn dump(&self) -> String {
        let mut d = Dumper {
            ast: self,
            out: String::new(),
            depth: 0,
        };
        // The standard library (`som`, `som::io`) is part of every program, and imported C
        // headers (`<c>`, or read from `.som/c.som`) are generated: neither is shown.
        let shown = |m: &&crate::Module| {
            let std = &*m.name == "som" || m.name.starts_with("som::");
            let generated = m.path.to_string_lossy().starts_with('<')
                || m.path.file_name().is_some_and(|n| n == ".som");
            !std && !generated
        };
        for module in self.modules.iter().filter(shown) {
            d.line(format!("module {:?}", &*module.name));
            d.nested(|d| module.items.iter().for_each(|item| d.item(item)));
        }
        d.out
    }

    /// A type annotation as it would be written in som.
    pub fn type_text(&self, id: Id<TypeExpr>) -> String {
        match &self.types[id] {
            TypeExpr::Unit => "()".into(),
            TypeExpr::Primitive(p) => format!("{p:?}").to_lowercase(),
            TypeExpr::Named { path, args } if args.is_empty() => path.to_string(),
            TypeExpr::Named { path, args } => {
                let args: Vec<String> = args.iter().map(|&a| self.type_text(a)).collect();
                format!("{path}<{}>", args.join(", "))
            }
            TypeExpr::Pointer => "*".into(),
            TypeExpr::PointerTo(to) => format!("*{}", self.type_text(*to)),
            TypeExpr::Reference {
                mutable,
                lifetime,
                to,
            } => {
                let lifetime = lifetime
                    .as_ref()
                    .map(|l| format!("'{l} "))
                    .unwrap_or_default();
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                let to = match &self.types[*to] {
                    TypeExpr::Optional(_) => format!("({})", self.type_text(*to)),
                    _ => self.type_text(*to),
                };
                format!("&{lifetime}{mutability}{to}")
            }
            TypeExpr::Array { element, len } => {
                format!("[{}; {}]", self.type_text(*element), self.type_text(*len))
            }
            TypeExpr::Int(n) => n.to_string(),
            TypeExpr::Bool(b) => b.to_string(),
            TypeExpr::Binary { op, lhs, rhs } => {
                // A sum inside a product keeps its parentheses.
                let side = |id: Id<TypeExpr>| match (&self.types[id], op) {
                    (TypeExpr::Binary { op: BinOp::Add, .. }, BinOp::Multiply) => {
                        format!("({})", self.type_text(id))
                    }
                    _ => self.type_text(id),
                };
                format!("{} {} {}", side(*lhs), op.symbol(), side(*rhs))
            }
            TypeExpr::Slice { element, mutable } => {
                let mutability = if *mutable { "mut " } else { "" };
                format!("[{mutability}{}]", self.type_text(*element))
            }
            // `fn() -> i32?` returns an optional.
            TypeExpr::Optional(inner) => match &self.types[*inner] {
                TypeExpr::Fn { .. } => format!("({})?", self.type_text(*inner)),
                _ => format!("{}?", self.type_text(*inner)),
            },
            TypeExpr::Fn { params, returns } => {
                let params: Vec<String> = params.iter().map(|&p| self.type_text(p)).collect();
                let returns = returns
                    .map(|r| format!(" -> {}", self.type_text(r)))
                    .unwrap_or_default();
                format!("fn({}){returns}", params.join(", "))
            }
        }
    }
}

struct Dumper<'a> {
    ast: &'a Ast,
    out: String,
    depth: usize,
}

impl Dumper<'_> {
    fn line(&mut self, text: impl AsRef<str>) {
        let _ = writeln!(
            self.out,
            "{:indent$}{}",
            "",
            text.as_ref(),
            indent = self.depth * 2
        );
    }

    fn nested(&mut self, f: impl FnOnce(&mut Self)) {
        self.depth += 1;
        f(self);
        self.depth -= 1;
    }

    /// A line with nested children.
    fn node(&mut self, text: impl AsRef<str>, f: impl FnOnce(&mut Self)) {
        self.line(text);
        self.nested(f);
    }

    fn item(&mut self, item: &Item) {
        let ast = self.ast;
        match item {
            Item::Func(id) => self.func(*id),
            Item::Struct(id) => {
                let s = &ast.structs[*id];
                let keyword = if s.is_enum() { "enum" } else { "struct" };
                self.node(format!("{keyword} {}", s.name), |d| {
                    for f in &s.fields {
                        d.line(format!("field {}: {}", f.name, ast.type_text(f.ty)));
                    }
                    for v in s.variants() {
                        d.node(format!("variant {}", v.name), |d| {
                            for f in &v.fields {
                                d.line(format!("field {}: {}", f.name, ast.type_text(f.ty)));
                            }
                        });
                    }
                });
            }
            Item::Use(u) => self.line(format!("use {}", u.path)),
            Item::Extern(block) => {
                let mut header = match &block.library {
                    Some(lib) => format!("extern {lib:?}"),
                    None => "extern".into(),
                };
                if block.use_module {
                    header.insert_str(0, "use ");
                }
                if let Some((file, _)) = &block.header {
                    header.push_str(&format!(" from {file:?}"));
                }
                self.node(header, |d| block.funcs.iter().for_each(|&f| d.func(f)));
            }
            Item::Impl(block) => {
                let header = match &block.trait_ {
                    Some((name, _)) => format!("impl {name} for {}", block.target),
                    None => format!("impl {}", block.target),
                };
                self.node(header, |d| {
                    for (name, ty) in &block.types {
                        d.line(format!("type {name} = {}", ast.type_text(*ty)));
                    }
                    block.methods.iter().for_each(|&f| d.func(f))
                })
            }
            Item::Trait(id) => {
                let t = &ast.traits[*id];
                self.node(format!("trait {}", t.name), |d| {
                    for assoc in &t.types {
                        d.line(format!("type {}", assoc.name));
                    }
                    t.methods.iter().for_each(|&f| d.func(f))
                })
            }
        }
    }

    fn func(&mut self, id: Id<Func>) {
        let ast = self.ast;
        let f = &ast.funcs[id];
        let generics = if f.generics.is_empty() {
            String::new()
        } else {
            let names: Vec<String> = (f.generics.iter())
                .map(|g| match g.ty {
                    Some(ty) => format!("{}: {}", g.name.name, ast.type_text(ty)),
                    None => g.name.name.to_string(),
                })
                .collect();
            format!("<{}>", names.join(", "))
        };
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| {
                let out = match (p.out, p.mutable) {
                    (true, _) => "out ",
                    (false, true) => "mut ",
                    (false, false) => "",
                };
                match p.ty {
                    Some(ty) => format!("{out}{}: {}", p.name, ast.type_text(ty)),
                    None => format!("{out}{}", p.name),
                }
            })
            .collect();
        let returns = f
            .returns
            .map(|r| format!(" -> {}", ast.type_text(r)))
            .unwrap_or_default();
        let params = if f.variadic {
            [params, vec!["...".to_string()]].concat()
        } else {
            params
        };
        self.node(
            format!("fn {}{generics}({}){returns}", f.name, params.join(", ")),
            |d| {
                if let Some(body) = f.body {
                    d.expr(body);
                }
            },
        );
    }

    fn stmts(&mut self, label: &str, stmts: &[Id<Stmt>]) {
        self.node(label, |d| stmts.iter().for_each(|&s| d.stmt(s)));
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        let ast = self.ast;
        match &ast.stmts[id] {
            Stmt::Use(u) => self.line(format!("use {}", u.path)),
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
            } => {
                let mutability = if *mutable { "mut " } else { "" };
                let ty = ty
                    .map(|t| format!(": {}", ast.type_text(t)))
                    .unwrap_or_default();
                self.node(format!("let {mutability}{name}{ty}"), |d| d.expr(*value));
            }
            Stmt::Expr(expr) => self.expr(*expr),
            Stmt::Loop { body } => self.stmts("loop", body),
            Stmt::Break(None) => self.line("break"),
            Stmt::Break(Some(name)) => self.line(format!("break {name}")),
            Stmt::Continue(None) => self.line("continue"),
            Stmt::Return(None) => self.line("return"),
            Stmt::Return(Some(value)) => self.node("return", |d| d.expr(*value)),
            Stmt::Continue(Some(name)) => self.line(format!("continue {name}")),
            Stmt::For {
                name,
                mutable,
                iterable,
                body,
            } => {
                let mutability = if *mutable { "mut " } else { "" };
                self.node(format!("for {mutability}{name}"), |d| {
                    d.expr(*iterable);
                    d.stmts("do", body);
                })
            }
            Stmt::While { condition, body } => self.node("while", |d| {
                d.expr(*condition);
                d.stmts("do", body);
            }),
            Stmt::If {
                condition,
                then,
                otherwise,
            } => self.node("if", |d| {
                d.expr(*condition);
                d.stmts("then", then);
                if let Some(otherwise) = otherwise {
                    d.stmts("else", otherwise);
                }
            }),
        }
    }

    fn exprs(&mut self, exprs: &[Id<Expr>]) {
        exprs.iter().for_each(|&e| self.expr(e));
    }

    fn expr(&mut self, id: Id<Expr>) {
        match &self.ast.exprs[id] {
            Expr::Hole => self.line("hole"),
            Expr::Int(n) => self.line(format!("int {n}")),
            Expr::Float(x) => self.line(format!("float {x:?}")),
            Expr::Bool(b) => self.line(format!("bool {b}")),
            Expr::Str(s) => self.line(format!("str {s:?}")),
            Expr::None => self.line("none"),
            Expr::Array(items) => self.node("array", |d| d.exprs(items)),
            Expr::Cast { value, ty } => {
                let ty = self.ast.type_text(*ty);
                self.node(format!("as {ty}"), |d| d.expr(*value))
            }
            Expr::Range { start, end } => self.node("range", |d| {
                match start {
                    Some(start) => d.node("start", |d| d.expr(*start)),
                    None => d.line("start open"),
                }
                match end {
                    Some(end) => d.node("end", |d| d.expr(*end)),
                    None => d.line("end open"),
                }
            }),
            Expr::ArrayRepeat { value, count } => {
                let count = self.ast.type_text(*count);
                self.node(format!("array repeat {count}"), |d| d.expr(*value))
            }
            Expr::Var(path) => self.line(format!("var {path}")),
            Expr::Binary { op, lhs, rhs } => self.node(format!("binary {}", op.symbol()), |d| {
                d.exprs(&[*lhs, *rhs])
            }),
            Expr::Not(e) => self.node("not", |d| d.expr(*e)),
            Expr::Neg(e) => self.node("neg", |d| d.expr(*e)),
            Expr::Borrow { mutable, expr } => self
                .node(if *mutable { "borrow mut" } else { "borrow" }, |d| {
                    d.expr(*expr)
                }),
            Expr::Deref(e) => self.node("deref", |d| d.expr(*e)),
            Expr::Out(name) => self.line(format!("out {name}")),
            Expr::Assign { target, value } => self.node("assign", |d| d.exprs(&[*target, *value])),
            Expr::Call { callee, args } => self.node(format!("call {callee}"), |d| d.exprs(args)),
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.node(format!("method {method}"), |d| {
                d.expr(*receiver);
                d.exprs(args);
            }),
            Expr::Field { object, field } => {
                self.node(format!("field {field}"), |d| d.expr(*object))
            }
            Expr::Index { object, index } => self.node("index", |d| {
                d.expr(*object);
                d.expr(*index);
            }),
            Expr::Construct { ty, fields, .. } => self.node(format!("construct {ty}"), |d| {
                for (name, value) in fields {
                    d.node(format!("{name}:"), |d| d.expr(*value));
                }
            }),
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => self.node("conditional", |d| {
                d.node("condition", |d| d.expr(*condition));
                d.node("then", |d| d.expr(*then));
                d.node("else", |d| d.expr(*otherwise));
            }),
            Expr::Fallback { optional, fallback } => {
                self.node("else", |d| d.exprs(&[*optional, *fallback]))
            }
            Expr::Match { scrutinee, arms } => self.node("match", |d| {
                d.expr(*scrutinee);
                for arm in arms {
                    let pattern = d.ast.pattern_text(arm.pattern);
                    d.node(format!("arm {pattern}"), |d| d.expr(arm.body));
                }
            }),
            Expr::Block { stmts, value } => self.node("block", |d| {
                stmts.iter().for_each(|&s| d.stmt(s));
                if let Some(value) = value {
                    d.node("value", |d| d.expr(*value));
                }
            }),
        }
    }
}

impl Ast {
    /// A pattern as it would be written.
    pub fn pattern_text(&self, id: Id<Pattern>) -> String {
        match &self.patterns[id] {
            Pattern::Wildcard => "_".into(),
            Pattern::Binding { name, mutable } => {
                format!("{}{name}", if *mutable { "mut " } else { "" })
            }
            Pattern::Int(n) => n.to_string(),
            Pattern::Bool(b) => b.to_string(),
            Pattern::None => "none".into(),
            Pattern::Variant { path, fields } => match fields {
                PatternFields::Unit => path.to_string(),
                PatternFields::Tuple(fields) => {
                    let fields: Vec<String> =
                        fields.iter().map(|&f| self.pattern_text(f)).collect();
                    format!("{path}({})", fields.join(", "))
                }
                PatternFields::Named { fields, rest } => {
                    let mut parts: Vec<String> = (fields.iter())
                        .map(|(name, p)| format!("{name}: {}", self.pattern_text(*p)))
                        .collect();
                    if *rest {
                        parts.push("..".into());
                    }
                    format!("{path} {{ {} }}", parts.join(", "))
                }
            },
        }
    }
}

impl BinOp {
    /// The operator as written in source.
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Subtract => "-",
            BinOp::Multiply => "*",
            BinOp::Divide => "/",
            BinOp::Modulo => "%",
            BinOp::LessThan => "<",
            BinOp::GreaterThan => ">",
            BinOp::LessThanOrEqual => "<=",
            BinOp::GreaterThanOrEqual => ">=",
            BinOp::Equals => "==",
            BinOp::NotEquals => "!=",
            BinOp::And => "and",
            BinOp::Or => "or",
        }
    }
}

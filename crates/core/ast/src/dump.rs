//! A stable text form of the syntax tree, for `som ... --emit ast`: one node per line,
//! children indented by two spaces. There are no spans or file paths, so dumps diff cleanly
//! between machines and between compiler implementations.

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
        // headers (`<c>`, or read from `.build/imports/c.som`) are generated: neither is shown.
        let shown = |m: &&crate::Module| {
            let std = som_lang::is_std_module(&m.name);
            let generated = m.path.to_string_lossy().starts_with('<')
                || m.path
                    .file_name()
                    .is_some_and(|n| n == som_lang::IMPORTS_DIR);
            !std && !generated
        };
        for module in self.modules.iter().filter(shown) {
            d.line(format!("module {:?}", &*module.name));
            d.nested(|d| module.items.iter().for_each(|item| d.item(item)));
        }
        d.out
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

    fn node(&mut self, text: impl AsRef<str>, f: impl FnOnce(&mut Self)) {
        self.line(text);
        self.nested(f);
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Func(id) => self.func(*id),
            Item::Struct(id) => self.struct_(*id),
            Item::Use(u) => self.line(format!("use {}", u.path)),
            Item::Alias(a) => self.alias(a),
            Item::Const(id) => {
                let c = &self.ast.consts[*id];
                let header = match c.ty {
                    Some(ty) => format!("@const {}: {}", c.name, self.ast.type_text(ty)),
                    None => format!("@const {}", c.name),
                };
                self.node(header, |d| d.expr(c.value))
            }
            Item::Extern(block) => self.extern_block(block),
            Item::Impl(block) => self.impl_block(block),
            Item::Trait(id) => self.trait_(*id),
        }
    }

    fn fields(&mut self, fields: &[Field]) {
        for f in fields {
            self.line(format!("field {}: {}", f.name, self.ast.type_text(f.ty)));
        }
    }

    fn struct_(&mut self, id: Id<Struct>) {
        let s = &self.ast.structs[id];
        let keyword = if s.is_enum() { "enum" } else { "struct" };
        self.node(format!("{keyword} {}", s.name), |d| {
            d.fields(&s.fields);
            for v in s.variants() {
                d.node(format!("variant {}", v.name), |d| d.fields(&v.fields));
            }
        });
    }

    fn alias(&mut self, a: &Alias) {
        let generics = match a.generics.is_empty() {
            true => String::new(),
            false => {
                let names: Vec<String> = a.generics.iter().map(|g| g.name.to_string()).collect();
                format!("<{}>", names.join(", "))
            }
        };
        let ty = self.ast.type_text(a.ty);
        self.line(format!("type {}{generics} = {ty}", a.name))
    }

    fn extern_block(&mut self, block: &Extern) {
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

    fn impl_block(&mut self, block: &Impl) {
        let header = match &block.trait_ {
            Some((name, _)) => format!("impl {name} for {}", block.target),
            None => format!("impl {}", block.target),
        };
        self.node(header, |d| {
            for (name, ty) in &block.types {
                d.line(format!("type {name} = {}", d.ast.type_text(*ty)));
            }
            block.methods.iter().for_each(|&f| d.func(f))
        })
    }

    fn trait_(&mut self, id: Id<Trait>) {
        let t = &self.ast.traits[id];
        self.node(format!("trait {}", t.name), |d| {
            for assoc in &t.types {
                d.line(format!("type {}", assoc.name));
            }
            t.methods.iter().for_each(|&f| d.func(f))
        })
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
        let mut params: Vec<String> = f.params.iter().map(|p| self.param(p)).collect();
        if f.variadic {
            params.push("...".to_string());
        }
        let returns = (f.returns)
            .map(|r| format!(" -> {}", ast.type_text(r)))
            .unwrap_or_default();
        let header = format!("fn {}{generics}({}){returns}", f.name, params.join(", "));
        self.node(header, |d| {
            if let Some(body) = f.body {
                d.expr(body);
            }
        });
    }

    fn param(&self, p: &Param) -> String {
        let out = match (p.out, p.mutable) {
            (true, _) => "out ",
            (false, true) => "mut ",
            (false, false) => "",
        };
        match p.ty {
            Some(ty) => format!("{out}{}: {}", p.name, self.ast.type_text(ty)),
            None => format!("{out}{}", p.name),
        }
    }

    fn stmts(&mut self, label: &str, stmts: &[Id<Stmt>]) {
        self.node(label, |d| stmts.iter().for_each(|&s| d.stmt(s)));
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        let ast = self.ast;
        match &ast.stmts[id] {
            Stmt::Use(u) => self.line(format!("use {}", u.path)),
            Stmt::Const(c) => self.line(format!("@const {}", ast.consts[*c].name)),
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
            Expr::Type(_) => self.line("type"),
            Expr::Fail(message) => self.node("@fail", |d| d.expr(*message)),
            Expr::InPlace(c) => {
                let value = self.ast.consts[*c].value;
                self.node("@const", |d| d.expr(value))
            }
            Expr::Int(n) => self.line(format!("int {n}")),
            Expr::Float(x) => self.line(format!("float {x:?}")),
            Expr::Bool(b) => self.line(format!("bool {b}")),
            Expr::Str(s) => self.line(format!("str {s:?}")),
            Expr::None => self.line("none"),
            Expr::Array(items) => self.node("array", |d| d.exprs(items)),
            Expr::Interpolated(parts) => self.node("interpolated", |d| {
                for part in parts {
                    match part {
                        crate::Part::Text(e) | crate::Part::Value(e) => d.expr(*e),
                    }
                }
            }),
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
            Expr::BitNot(e) => self.node("bitnot", |d| d.expr(*e)),
            Expr::Unreachable => self.line("unreachable"),
            Expr::Borrow { mutable, expr } => self
                .node(if *mutable { "borrow mut" } else { "borrow" }, |d| {
                    d.expr(*expr)
                }),
            Expr::Deref(e) => self.node("deref", |d| d.expr(*e)),
            Expr::Out(name) => self.line(format!("out {name}")),
            Expr::Assign { target, value } => self.node("assign", |d| d.exprs(&[*target, *value])),
            Expr::Call { callee, args, .. } => {
                self.node(format!("call {callee}"), |d| d.exprs(args))
            }
            Expr::Closure { params, body } => {
                let names: Vec<&str> = params.iter().map(|p| &*p.name.name).collect();
                self.node(format!("closure ({})", names.join(", ")), |d| d.expr(*body))
            }
            Expr::CallValue { callee, args } => self.node("call value", |d| {
                d.expr(*callee);
                d.exprs(args);
            }),
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

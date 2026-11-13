//! Renders MIR as text, close to Rust's MIR dumps:
//!
//! ```text
//! fn add(_1: i32, _2: i32) -> i32 {
//!     let _0: i32;
//!     let _1: i32; // a
//!     let _2: i32; // b
//!
//!     bb0: {
//!         _0 = Add(copy _1, copy _2);
//!         return;
//!     }
//! }
//! ```

use std::fmt::{self, Display, Write};

use crate::*;

impl Mir {
    /// Renders a type the way it would be written in som.
    pub fn display_ty(&self, ty: TypeId) -> String {
        match self.types.get(ty) {
            Ty::Struct(def) => self.struct_def(*def).name.clone(),
            Ty::Ref { mutable, to } => {
                let mutability = if *mutable { "mut " } else { "" };
                // `&i32?` is an optional reference.
                match self.types.get(*to) {
                    Ty::Optional(_) => format!("&{mutability}({})", self.display_ty(*to)),
                    Ty::Slice(element) => format!("[{mutability}{}]", self.display_ty(*element)),
                    _ => format!("&{mutability}{}", self.display_ty(*to)),
                }
            }
            // `fn() -> i32?` returns an optional.
            Ty::Optional(inner) => match self.types.get(*inner) {
                Ty::Fn { .. } => format!("({})?", self.display_ty(*inner)),
                _ => format!("{}?", self.display_ty(*inner)),
            },
            Ty::Array { element, len } => format!("[{}; {len}]", self.display_ty(*element)),
            Ty::Slice(element) => format!("[{}]", self.display_ty(*element)),
            Ty::Fn { params, ret } => {
                let params: Vec<String> = params.iter().map(|&p| self.display_ty(p)).collect();
                format!("fn({}) -> {}", params.join(", "), self.display_ty(*ret))
            }
            Ty::Error => "{error}".into(),
            Ty::Unit => "()".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "char".into(),
            Ty::Str => "str".into(),
            Ty::Int(int) => format!("{int:?}").to_lowercase(),
            Ty::Float(float) => format!("{float:?}").to_lowercase(),
            Ty::Pointer => "*".into(),
            Ty::Param(_) | Ty::Var(_) => unreachable!("MIR only has concrete types"),
        }
    }

    pub fn display_func(&self, id: FuncId) -> String {
        let mut out = String::new();
        self.write_func(&mut out, self.func(id)).unwrap();
        out
    }

    fn write_func(&self, out: &mut String, func: &Function) -> fmt::Result {
        let Some(body) = &func.body else {
            let mut params: Vec<String> = func.params.iter().map(|&p| self.display_ty(p)).collect();
            if func.variadic {
                params.push("...".into());
            }
            let link = func.link_name.as_deref().unwrap_or("?");
            return writeln!(
                out,
                "extern fn {}({}) -> {}; // links `{link}`",
                func.name,
                params.join(", "),
                self.display_ty(func.ret)
            );
        };

        let params: Vec<String> = body
            .args()
            .map(|l| format!("{l}: {}", self.display_ty(body.local(l).ty)))
            .collect();
        writeln!(
            out,
            "fn {}({}) -> {} {{",
            func.name,
            params.join(", "),
            self.display_ty(func.ret)
        )?;

        for (id, local) in body.locals.iter_with_ids() {
            let mutability = if local.mutable { "mut " } else { "" };
            write!(
                out,
                "    let {mutability}{}: {};",
                id,
                self.display_ty(local.ty)
            )?;
            if let Some(name) = &local.name {
                write!(out, " // {name}")?;
            }
            writeln!(out)?;
        }

        for (id, block) in body.blocks.iter_with_ids() {
            writeln!(out, "\n    {id}: {{")?;
            for stmt in &block.stmts {
                writeln!(out, "        {};", self.display_stmt(&stmt.kind))?;
            }
            writeln!(
                out,
                "        {};",
                self.display_terminator(&block.terminator.kind)
            )?;
            writeln!(out, "    }}")?;
        }
        writeln!(out, "}}")
    }

    fn display_stmt(&self, stmt: &StatementKind) -> String {
        match stmt {
            StatementKind::Assign(place, rvalue) => {
                format!("{place} = {}", self.display_rvalue(rvalue))
            }
            StatementKind::StorageLive(local) => format!("StorageLive({local})"),
            StatementKind::StorageDead(local) => format!("StorageDead({local})"),
        }
    }

    fn display_terminator(&self, terminator: &TerminatorKind) -> String {
        match terminator {
            TerminatorKind::Goto(block) => format!("goto -> {block}"),
            TerminatorKind::Branch {
                condition,
                then,
                otherwise,
            } => {
                format!(
                    "branch({}) -> [true: {then}, false: {otherwise}]",
                    self.display_operand(condition)
                )
            }
            TerminatorKind::Return => "return".into(),
            TerminatorKind::Unreachable => "unreachable".into(),
        }
    }

    fn display_rvalue(&self, rvalue: &Rvalue) -> String {
        match rvalue {
            Rvalue::Use(op) => self.display_operand(op),
            Rvalue::Ref { mutable, place } => {
                format!("&{}{place}", if *mutable { "mut " } else { "" })
            }
            Rvalue::Binary { op, lhs, rhs } => format!(
                "{op:?}({}, {})",
                self.display_operand(lhs),
                self.display_operand(rhs)
            ),
            Rvalue::Not(op) => format!("Not({})", self.display_operand(op)),
            Rvalue::Some(op) => format!("Some({})", self.display_operand(op)),
            Rvalue::Array { items, .. } => {
                let items: Vec<String> = items.iter().map(|op| self.display_operand(op)).collect();
                format!("[{}]", items.join(", "))
            }
            Rvalue::Repeat { ty, operand } => {
                let len = match self.types.get(*ty) {
                    Ty::Array { len, .. } => *len,
                    _ => 0,
                };
                format!("[{}; {len}]", self.display_operand(operand))
            }
            Rvalue::Cast { operand, ty } => {
                format!(
                    "{} as {}",
                    self.display_operand(operand),
                    self.display_ty(*ty)
                )
            }
            Rvalue::Struct { def, fields } => {
                let s = self.struct_def(*def);
                let fields: Vec<String> = s
                    .fields
                    .iter()
                    .zip(fields)
                    .map(|((name, _), op)| format!("{name}: {}", self.display_operand(op)))
                    .collect();
                format!("{} {{ {} }}", s.name, fields.join(", "))
            }
            Rvalue::Call { func, args } => {
                let args: Vec<String> = args.iter().map(|a| self.display_operand(a)).collect();
                format!("call {}({})", self.func(*func).name, args.join(", "))
            }
        }
    }

    fn display_operand(&self, op: &Operand) -> String {
        match op {
            Operand::Copy(place) => format!("copy {place}"),
            Operand::Move(place) => format!("move {place}"),
            Operand::Const(c) => match &c.kind {
                ConstKind::Int(n) => format!("const {n}_{}", self.display_ty(c.ty)),
                ConstKind::Float(x) => format!("const {x:?}_{}", self.display_ty(c.ty)),
                ConstKind::Bool(b) => format!("const {b}"),
                ConstKind::Str(s) => format!("const {s:?}"),
                ConstKind::Unit => "const ()".into(),
                ConstKind::Func(f) => format!("const {}", self.func(*f).name),
                ConstKind::Zeroed => format!("const zeroed_{}", self.display_ty(c.ty)),
            },
        }
    }
}

impl Display for Mir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        for (i, func) in self.funcs.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            self.write_func(&mut out, func)?;
        }
        f.write_str(&out)
    }
}

impl Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Derefs wrap everything before them: `(*_1).0`.
        let mut s = self.local.to_string();
        for projection in &self.projection {
            s = match projection {
                Projection::Field(i) => format!("{s}.{i}"),
                Projection::Deref => format!("(*{s})"),
                Projection::Index(index) => format!("{s}[{index}]"),
            };
        }
        f.write_str(&s)
    }
}

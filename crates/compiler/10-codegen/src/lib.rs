//! Code generation: MIR to C.
//!
//! The output is a single self-contained C11 file, written to read like the
//! program it came from:
//!
//! | MIR                         | C                                                      |
//! |-----------------------------|--------------------------------------------------------|
//! | struct `geo::Point`         | `struct geo_Point { int32_t x; int32_t y; }`           |
//! | function `Point::sum`       | `static int32_t Point_sum(struct Point* self)`         |
//! | `&mut T` parameter          | `T* restrict`                                          |
//! | local                       | its som name, declared where first assigned            |
//! | temporary holding a call    | named after the function: `x = Vector_x(&v)`           |
//! | other temporary             | `_N`, if it isn't folded away                          |
//! | block `bbN`                 | a label, if anything jumps to it                       |
//! | place `(*self).0`           | `self->x`                                              |
//! | `StorageLive`/`StorageDead` | nothing                                                |
//!
//! A temporary that's used once is folded into that use: `_2 = a + b; _0 = _2;
//! return _0;` becomes `return a + b;`. It can wait past statements that don't
//! assign what it reads or write memory it reads; one holding a call can't
//! wait at all, so effects stay in order. C leaves the order of evaluation
//! within an expression open, and does arithmetic on small integer types in
//! `int`, so folding is limited to where neither can change the meaning (see
//! [`Use::allows`](fold::Use::allows)). Operands are parenthesized only where C's precedence
//! would group them differently.
//!
//! The C compiler lays out structs and handles calling conventions, so neither
//! appears here.

mod arith;
mod body;
mod c_types;
mod expr;
mod fold;
mod names;
mod runtime;
mod types;

use names::Names;
use som_mir::*;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodegenError {
    NoEntryPoint,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodegenError::NoEntryPoint => f.write_str("the program has no `main` function"),
        }
    }
}

pub fn emit_c(mir: &Mir) -> Result<String, CodegenError> {
    let (entry, _) = mir
        .funcs
        .iter_with_ids()
        .find(|(_, f)| f.is_entry)
        .ok_or(CodegenError::NoEntryPoint)?;
    let names = Names::new(mir);
    let mut c = Emitter {
        mir,
        names,
        out: String::new(),
        helpers: RefCell::new(BTreeMap::new()),
    };
    c.prelude();
    c.structs();
    c.declarations();
    // The helpers the functions use go before them, once they're known.
    let helpers_at = c.out.len();
    for (id, func) in c.cpu_funcs() {
        if let Some(body) = &func.body {
            c.function(id, func, body);
        }
    }
    c.main(entry);
    let helpers: String = c.helpers.borrow().values().cloned().collect();
    if !helpers.is_empty() {
        c.out.insert_str(helpers_at, &format!("{helpers}\n"));
    }
    Ok(c.out)
}

struct Emitter<'a> {
    mir: &'a Mir,
    names: Names,
    out: String,
    /// Definitions the code needs besides the program's, by name: shift helpers and the
    /// blocks of `string` literals.
    helpers: RefCell<BTreeMap<String, String>>,
}

impl<'a> Emitter<'a> {
    /// The functions the program runs: not the copies compiled for the GPU, which `@glsl`
    /// has already made GLSL of.
    fn cpu_funcs(&self) -> impl Iterator<Item = (FuncId, &'a Function)> + use<'a> {
        let mir = self.mir;
        (mir.funcs.iter_with_ids()).filter(move |(id, _)| !mir.gpu.contains(id))
    }

    fn line(&mut self, text: impl AsRef<str>) {
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn func_name(&self, id: FuncId) -> &str {
        &self.names.funcs[&id]
    }

    fn signature(&self, id: FuncId, func: &Function, params: Option<&[String]>) -> String {
        let params: Vec<String> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let mut ty = self.ty(p);
                // The borrow checker makes sure nothing else reaches what a `&mut`
                // parameter points to while the call runs, which is what `restrict`
                // promises C. Extern functions keep the signature C declared.
                if func.link_name.is_none()
                    && let Ty::Ref { mutable: true, to } = self.mir.types.get(p)
                    && !matches!(self.mir.types.get(*to), Ty::Slice(_))
                {
                    ty.push_str(" restrict");
                }
                match params {
                    Some(names) => format!("{ty} {}", names[i + 1]),
                    None => ty,
                }
            })
            .collect();
        let mut params = if params.is_empty() {
            "void".into()
        } else {
            params.join(", ")
        };
        if func.variadic {
            params.push_str(", ...");
        }
        let storage = if func.link_name.is_some() {
            ""
        } else {
            "static "
        };
        format!(
            "{storage}{} {}({params})",
            self.ret_ty(func),
            self.func_name(id)
        )
    }

    /// The C functions the program calls, then its own functions, each with its som name.
    fn declarations(&mut self) {
        let mut externs = HashSet::new();
        let mut any = false;
        for (id, func) in self.cpu_funcs() {
            // Two modules can declare the same C function; declare it once.
            if let Some(link) = &func.link_name
                && externs.insert(link.clone())
            {
                let decl = self.signature(id, func, None);
                self.line(format!("{decl};"));
                any = true;
            }
        }
        if any {
            self.line("");
        }
        for (id, func) in self.cpu_funcs() {
            if func.link_name.is_none() {
                let decl = self.signature(id, func, None);
                self.line(format!("{decl}; // {}", func.name));
            }
        }
        self.line("");
    }

    fn main(&mut self, entry: FuncId) {
        let func = self.mir.func(entry);
        let call = format!("{}()", self.func_name(entry));
        self.line("int main(int argc, char** argv) {");
        self.line("    som_argc = argc;");
        self.line("    som_argv = argv;");
        if self.is_unit(func.ret) {
            self.line(format!("    {call};"));
            self.line("    return 0;");
        } else {
            self.line(format!("    return (int){call};"));
        }
        self.line("}");
    }
}

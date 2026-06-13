use std::fmt;

use som_common::{Id, LineWriter, Pretty, Show, SourceMap};
use som_hir::TyCtx;

use crate::{Block, Const, Function, LocalDecl, Operand, Rvalue, Statement, Terminator};

#[derive(Copy, Clone)]
pub struct MirCtx<'a> {
    pub tcx: &'a TyCtx,
    pub sources: Option<&'a SourceMap>,
}

impl Function {
    pub fn display<'a>(&'a self, tcx: &'a TyCtx) -> Show<'a, Function, MirCtx<'a>> {
        Show::new(self, MirCtx { tcx, sources: None })
    }

    pub fn display_with_sources<'a>(
        &'a self,
        tcx: &'a TyCtx,
        sources: &'a SourceMap,
    ) -> Show<'a, Function, MirCtx<'a>> {
        Show::new(
            self,
            MirCtx {
                tcx,
                sources: Some(sources),
            },
        )
    }

    fn local_name(&self, id: Id<LocalDecl>) -> String {
        format!("{}_{}", self.locals[id].name, id.id)
    }

    fn block_name(&self, id: Id<Block>) -> String {
        format!("{}_{}", self.blocks[id].name, id.id)
    }
}

impl Pretty<MirCtx<'_>> for Function {
    fn pretty(&self, ctx: MirCtx<'_>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut w = LineWriter::new(f, ctx.sources);

        let ret = match self.return_local {
            Some(id) => self.local_name(id),
            None => "()".to_string(),
        };
        w.line(None, 0, format!("fn main() -> {ret} {{"))?;

        for (id, local) in self.locals.iter_with_ids() {
            w.line(
                Some(local.span),
                4,
                format!("let {}: {};", self.local_name(id), ctx.tcx[local.ty]),
            )?;
        }
        if !self.locals.is_empty() {
            w.blank()?;
        }

        let mut first = true;
        for (block_id, block) in self.blocks.iter_with_ids() {
            if !first {
                w.blank()?;
            }
            first = false;
            w.line(None, 4, format!("{}: {{", self.block_name(block_id)))?;
            for stmt_id in &block.stmts {
                let stmt = &self.statements[*stmt_id];
                let mut buf = String::new();
                fmt_stmt(&mut buf, self, stmt);
                buf.push(';');
                w.line(Some(stmt.span()), 8, buf)?;
            }
            let mut term = String::new();
            fmt_term(&mut term, self, &block.terminator);
            term.push(';');
            w.line(None, 8, term)?;
            w.line(None, 4, "}")?;
        }

        w.line(None, 0, "}")
    }
}

fn fmt_stmt(buf: &mut String, func: &Function, stmt: &Statement) {
    use std::fmt::Write;
    match stmt {
        Statement::Assign { local, rvalue, .. } => {
            let _ = write!(buf, "{} = ", func.local_name(*local));
            fmt_rvalue(buf, func, rvalue);
        }
    }
}

fn fmt_rvalue(buf: &mut String, func: &Function, rv: &Rvalue) {
    use std::fmt::Write;
    match rv {
        Rvalue::Use(op) => fmt_operand(buf, func, op),
        Rvalue::UnaryOp(op, operand) => {
            let _ = write!(buf, "{op} ");
            fmt_operand(buf, func, operand);
        }
        Rvalue::BinaryOp(l, op, r) => {
            fmt_operand(buf, func, l);
            let _ = write!(buf, " {op} ");
            fmt_operand(buf, func, r);
        }
    }
}

fn fmt_operand(buf: &mut String, func: &Function, op: &Operand) {
    use std::fmt::Write;
    match op {
        Operand::Copy(id) => {
            let _ = write!(buf, "copy {}", func.local_name(*id));
        }
        Operand::Const(c) => match c {
            Const::Int(v, _) => {
                let _ = write!(buf, "const {v}_i32");
            }
            Const::Bool(b, _) => {
                let _ = write!(buf, "const {b}");
            }
        },
    }
}

fn fmt_term(buf: &mut String, func: &Function, t: &Terminator) {
    use std::fmt::Write;
    match t {
        Terminator::Return => match func.return_local {
            Some(id) => {
                let _ = write!(buf, "return {}", func.local_name(id));
            }
            None => {
                let _ = buf.write_str("return");
            }
        },
        Terminator::Goto(id) => {
            let _ = write!(buf, "goto -> {}", func.block_name(*id));
        }
        Terminator::SwitchInt { discr, targets } => {
            let _ = buf.write_str("switch ");
            fmt_operand(buf, func, discr);
            let _ = buf.write_str(" -> [");
            for (i, (value, target)) in targets.iter().enumerate() {
                if i > 0 {
                    let _ = buf.write_str(", ");
                }
                let _ = write!(buf, "{value}: {}", func.block_name(*target));
            }
            let _ = buf.write_str("]");
        }
        Terminator::Unreachable => {
            let _ = buf.write_str("unreachable");
        }
    }
}

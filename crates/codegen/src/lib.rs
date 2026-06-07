use cranelift_codegen::{
    ir::{AbiParam, InstBuilder, TrapCode, Value, types},
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use som_hir::{TyCtx, UnaryOp};
use som_mir::{Const, Function as MirFunction, Operand, Rvalue, Statement, Terminator};

pub fn codegen(mir: &MirFunction, tcx: &TyCtx) -> Result<fn() -> i32, String> {
    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").unwrap();
    flag_builder.set("is_pic", "false").unwrap();

    let isa_builder = cranelift_native::builder().map_err(|e| e.to_string())?;
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .map_err(|e| e.to_string())?;

    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    let mut module = JITModule::new(builder);

    let mut ctx = module.make_context();
    let mut fb_ctx = FunctionBuilderContext::new();

    // `fn main() -> i32` — the only signature we emit for now.
    ctx.func.signature.returns.push(AbiParam::new(types::I32));

    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);
        lower_function(mir, tcx, &mut builder);
        builder.finalize();
    }

    if std::env::var("SOM_DUMP_CODEGEN").is_ok() {
        som_common::info!("IR dump:\n{}", ctx.func.display());
    }

    let func_id = module
        .declare_function("main", Linkage::Export, &ctx.func.signature)
        .map_err(|e| e.to_string())?;
    module
        .define_function(func_id, &mut ctx)
        .map_err(|e| e.to_string())?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().map_err(|e| e.to_string())?;

    let func_ptr = module.get_finalized_function(func_id);
    let func: fn() -> i32 = unsafe { std::mem::transmute(func_ptr) };

    // Leak the module so the JIT-compiled code stays mapped for the function's
    // lifetime. Fine for our toy run-once usage.
    std::mem::forget(module);

    Ok(func)
}

fn lower_function(mir: &MirFunction, tcx: &TyCtx, b: &mut FunctionBuilder) {
    // Map MIR local id (dense u32) → Cranelift Variable.
    let local_vars: Vec<Variable> = mir
        .locals
        .iter_with_ids()
        .map(|(_, local)| b.declare_var(lower_type(&tcx[local.ty])))
        .collect();

    // One Cranelift block per MIR block, indexed parallel to the arena.
    let clif_blocks: Vec<_> = mir
        .blocks
        .iter_with_ids()
        .map(|_| b.create_block())
        .collect();

    let entry = clif_blocks[mir.entry.id];
    b.append_block_params_for_function_params(entry);

    for (mir_id, mir_block) in mir.blocks.iter_with_ids() {
        let clif_block = clif_blocks[mir_id.id];
        b.switch_to_block(clif_block);

        for stmt_id in &mir_block.stmts {
            match &mir.statements[*stmt_id] {
                Statement::Assign { local, rvalue, .. } => {
                    let v = lower_rvalue(b, &local_vars, rvalue);
                    b.def_var(local_vars[local.id], v);
                }
            }
        }

        match &mir_block.terminator {
            Terminator::Return => {
                let val = match mir.return_local {
                    Some(id) => {
                        let v = b.use_var(local_vars[id.id]);
                        if b.func.dfg.value_type(v) == types::I32 {
                            v
                        } else {
                            b.ins().uextend(types::I32, v) // cast return type to i32
                        }
                    }
                    None => b.ins().iconst(types::I32, 0),
                };
                b.ins().return_(&[val]);
            }
            Terminator::Goto(target) => {
                b.ins().jump(clif_blocks[target.id], &[]);
            }
            Terminator::Unreachable => {
                b.ins().trap(TrapCode::user(1).unwrap());
            }
        }
    }

    // No back-edges yet, so it's safe to seal everything at the end.
    b.seal_all_blocks();
}

fn lower_rvalue(b: &mut FunctionBuilder, locals: &[Variable], rv: &Rvalue) -> Value {
    use som_hir::BinaryOp;
    match rv {
        Rvalue::Use(op) => lower_operand(b, locals, op),
        Rvalue::UnaryOp(op, operand) => {
            let operand = lower_operand(b, locals, operand);
            match op {
                UnaryOp::Negate => b.ins().ineg(operand),
                UnaryOp::Not => b.ins().bxor_imm(operand, 1), // bool negation: x ^ 1
            }
        }
        Rvalue::BinaryOp(l, op, r) => {
            let lv = lower_operand(b, locals, l);
            let rv = lower_operand(b, locals, r);
            match op {
                BinaryOp::Add => b.ins().iadd(lv, rv),
                BinaryOp::Subtract => b.ins().isub(lv, rv),
                BinaryOp::Multiply => b.ins().imul(lv, rv),
                BinaryOp::Divide => b.ins().sdiv(lv, rv),
            }
        }
    }
}

fn lower_operand(b: &mut FunctionBuilder, locals: &[Variable], op: &Operand) -> Value {
    match op {
        Operand::Copy(id) => b.use_var(locals[id.id]),
        Operand::Const(Const::Int(v, _)) => b.ins().iconst(types::I32, *v),
        Operand::Const(Const::Bool(v, _)) => b.ins().iconst(types::I8, if *v { 1 } else { 0 }),
    }
}

fn lower_type(ty: &som_hir::Type) -> types::Type {
    match ty {
        som_hir::Type::Int { .. } => types::I32,
        som_hir::Type::Bool { .. } => types::I8,
        som_hir::Type::Error { .. } => unreachable!("error type should not reach codegen"),
    }
}

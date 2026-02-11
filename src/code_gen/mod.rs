use crate::{
    BinOp, Diagnostic, Expr, ExternFunc, Func, Stmt, Type, TypedAst, arena::Id,
    scope::ScopedEnvironment,
};
use cranelift::{
    codegen::ir::Function,
    module::*,
    object::*,
    prelude::{settings::Flags, *},
};
use std::collections::HashMap;
use std::sync::Arc;
use target_lexicon::Triple;

type Result<T> = std::result::Result<T, Diagnostic>;

enum CodegenError {
    NoMainFunction,
    NoReturnType,
    UnsupportedTarget(String),
    ModuleError(String),
}

impl CodegenError {
    fn to_diagnostic(self) -> Diagnostic {
        match self {
            CodegenError::NoMainFunction => {
                Diagnostic::error("no `main` function found".to_string())
                    .with_hint("add a `main` function as the entry point")
            }
            CodegenError::NoReturnType => {
                Diagnostic::error("`main` function must have a return type".to_string())
            }
            CodegenError::UnsupportedTarget(target) => {
                Diagnostic::error(format!("unsupported target: {}", target))
            }
            CodegenError::ModuleError(msg) => {
                Diagnostic::error(format!("code generation failed: {}", msg))
            }
        }
    }
}

pub struct Codegen<'ast> {
    isa: Arc<dyn isa::TargetIsa>,
    typed_ast: &'ast TypedAst,
    module: ObjectModule,
    func_ids: Vec<cranelift::module::FuncId>,
    extern_func_ids: HashMap<String, cranelift::module::FuncId>,
}

impl<'ast> Codegen<'ast> {
    pub fn new(typed_ast: &'ast TypedAst, triple: Triple) -> Result<Self> {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "true").unwrap();

        let triple_str = triple.to_string();
        let isa = isa::lookup(triple)
            .map_err(|_| CodegenError::UnsupportedTarget(triple_str).to_diagnostic())?
            .finish(Flags::new(flag_builder))
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;

        let builder = ObjectBuilder::new(isa.clone(), "main", default_libcall_names())
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;
        let module = ObjectModule::new(builder);

        Ok(Self {
            typed_ast,
            isa,
            module,
            func_ids: vec![],
            extern_func_ids: HashMap::new(),
        })
    }

    pub fn compile(mut self) -> Result<ObjectProduct> {
        for func in &self.typed_ast.ast.funcs {
            self.dec_func(func)?;
        }

        for extern_func in &self.typed_ast.ast.extern_funcs {
            self.dec_extern_func(extern_func)?;
        }

        for func in &self.typed_ast.ast.funcs {
            self.gen_func(func)?;
        }

        Ok(self.module.finish())
    }

    fn dec_func(&mut self, func: &Func) -> Result<()> {
        let sig = self.build_signature(func);
        let linkage = if func.name.value == "main".into() {
            Linkage::Export
        } else {
            Linkage::Local
        };

        let func_id = self
            .module
            .declare_function(&func.name.value, linkage, &sig)
            .map_err(|e| {
                CodegenError::ModuleError(format!(
                    "failed to declare function {}: {}",
                    func.name.value, e
                ))
                .to_diagnostic()
            })?;

        self.func_ids.push(func_id);

        Ok(())
    }

    fn dec_extern_func(&mut self, func: &ExternFunc) -> Result<()> {
        let sig = self.build_extern_signature(func);
        let func_id = self
            .module
            .declare_function(&func.name.value, Linkage::Import, &sig)
            .map_err(|e| {
                CodegenError::ModuleError(format!(
                    "failed to declare extern function {}: {}",
                    func.name.value, e
                ))
                .to_diagnostic()
            })?;

        self.extern_func_ids
            .insert(func.name.value.to_string(), func_id);

        Ok(())
    }

    fn gen_func(&mut self, func: &Func) -> Result<()> {
        let return_type = func
            .return_type
            .as_ref()
            .ok_or_else(|| CodegenError::NoReturnType.to_diagnostic())?;

        let mut ctx = self.module.make_context();

        // function signature
        let mut sig = Signature::new(self.isa.default_call_conv());
        sig.params = func
            .parameters
            .iter()
            .filter_map(|p| p.ty.as_ref().map(|ty| AbiParam::new(to_type(ty))))
            .collect();
        sig.returns = vec![AbiParam::new(to_type(return_type))];
        ctx.func.signature = sig.clone();

        let mut func_builder_ctx = FunctionBuilderContext::new();
        let mut func_ctx = FuncCtx::new(&mut ctx.func, &mut func_builder_ctx);

        let entry_block = func_ctx.body.create_block();
        func_ctx
            .body
            .append_block_params_for_function_params(entry_block);
        func_ctx.body.switch_to_block(entry_block);
        func_ctx.body.seal_block(entry_block);

        let block_params: Vec<Value> = func_ctx.body.block_params(entry_block).to_vec();
        for (i, param) in func.parameters.iter().enumerate() {
            if let Some(ty) = &param.ty {
                let param_value = block_params[i];
                let var = func_ctx.body.declare_var(to_type(ty));
                func_ctx.body.def_var(var, param_value);
                func_ctx.env.insert(param.name.value.to_string(), var);
            }
        }

        let return_value = self.gen_expr(&mut func_ctx, func.body);
        func_ctx.body.ins().return_(&[return_value]);

        // Print Cranelift IR
        println!("{}", ctx.func);

        let func_id = self
            .module
            .declare_function(&func.name.value, Linkage::Export, &sig)
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;
        self.module
            .define_function(func_id, &mut ctx)
            .map_err(|e| CodegenError::ModuleError(e.to_string()).to_diagnostic())?;

        self.module.clear_context(&mut ctx);

        Ok(())
    }

    fn gen_expr(&mut self, func: &mut FuncCtx, expr_id: Id<Expr>) -> Value {
        let expr = self.typed_ast.ast.exprs.get(&expr_id);

        match expr {
            Expr::Hole => unreachable!("parser should have thrown an error"),
            Expr::I32(v) => func.body.ins().iconst(to_type(&Type::I32), *v as i64),
            Expr::Bool(v) => func
                .body
                .ins()
                .iconst(to_type(&Type::Bool), if *v { 1 } else { 0 }),
            Expr::String(v) => {
                let data = self
                    .module
                    .declare_data(&format!("str_{}", v), Linkage::Local, false, false)
                    .unwrap();

                let mut data_description = DataDescription::new();
                let mut bytes = v.as_bytes().to_vec();
                bytes.push(0); // null terminator for C strings

                data_description.define(bytes.into_boxed_slice());
                self.module.define_data(data, &data_description).unwrap();

                let value = self.module.declare_data_in_func(data, func.body.func);
                func.body.ins().global_value(to_type(&Type::Str), value)
            }
            Expr::Var(ident) => {
                let var = func.env.get(&ident.value).expect("variable not found");

                func.body.use_var(*var)
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs_val = self.gen_expr(func, *lhs);
                let rhs_val = self.gen_expr(func, *rhs);

                match op {
                    BinOp::Add => func.body.ins().iadd(lhs_val, rhs_val),
                    BinOp::Subtract => func.body.ins().isub(lhs_val, rhs_val),
                    BinOp::Multiply => func.body.ins().imul(lhs_val, rhs_val),
                    BinOp::Divide => func.body.ins().sdiv(lhs_val, rhs_val),
                    // Comparisons return i8 (bool)
                    BinOp::LessThan => {
                        func.body
                            .ins()
                            .icmp(IntCC::SignedLessThan, lhs_val, rhs_val)
                    }
                    BinOp::GreaterThan => {
                        func.body
                            .ins()
                            .icmp(IntCC::SignedGreaterThan, lhs_val, rhs_val)
                    }
                    BinOp::LessThanOrEqual => {
                        func.body
                            .ins()
                            .icmp(IntCC::SignedLessThanOrEqual, lhs_val, rhs_val)
                    }
                    BinOp::GreaterThanOrEqual => {
                        func.body
                            .ins()
                            .icmp(IntCC::SignedGreaterThanOrEqual, lhs_val, rhs_val)
                    }
                    BinOp::Equals => func.body.ins().icmp(IntCC::Equal, lhs_val, rhs_val),
                    BinOp::NotEquals => func.body.ins().icmp(IntCC::NotEqual, lhs_val, rhs_val),
                    // Boolean ops
                    BinOp::And => func.body.ins().band(lhs_val, rhs_val),
                    BinOp::Or => func.body.ins().bor(lhs_val, rhs_val),
                }
            }
            Expr::Block { stmts, value } => {
                func.env.enter_scope();

                for stmt in stmts {
                    self.gen_stmt(func, *stmt);
                }

                let result = match value {
                    Some(expr_id) => self.gen_expr(func, *expr_id),
                    None => func.body.ins().iconst(to_type(&Type::Unit), 0), // TODO: unit shouldn't be a real value
                };

                func.env.leave_scope();
                result
            }
            Expr::Call { name, args } => {
                // Check extern functions first (matches type checker order)
                let callee_func_id =
                    if let Some(&id) = self.extern_func_ids.get(&*name.value) {
                        id
                    } else {
                        // Fall back to regular function lookup
                        let func_id = self
                            .typed_ast
                            .ast
                            .find_func_by_name(&name.value)
                            .expect("type checker should have caught unknown function");
                        self.func_ids[func_id.id]
                    };

                let callee_func_ref = self
                    .module
                    .declare_func_in_func(callee_func_id, func.body.func);

                let arguments: Vec<Value> =
                    args.iter().map(|arg| self.gen_expr(func, *arg)).collect();

                let call = func.body.ins().call(callee_func_ref, &arguments);

                func.body.inst_results(call)[0]
            }
            Expr::Borrow { mutable, expr } => todo!(),
            Expr::Deref { expr } => todo!(),
            Expr::Conditional {
                condition,
                truthy,
                falsy,
            } => {
                let condition_val = self.gen_expr(func, *condition);

                let truthy_block = func.body.create_block();
                let falsy_block = func.body.create_block();
                let merge_block = func.body.create_block();

                func.body
                    .ins()
                    .brif(condition_val, truthy_block, &[], falsy_block, &[]);

                // truthy
                func.body.switch_to_block(truthy_block);
                let truthy_val = self.gen_expr(func, *truthy);
                func.body.ins().jump(merge_block, &[truthy_val.into()]);
                func.body.seal_block(truthy_block);

                // falsy
                func.body.switch_to_block(falsy_block);
                let falsy_val = self.gen_expr(func, *falsy);
                func.body.ins().jump(merge_block, &[falsy_val.into()]);
                func.body.seal_block(falsy_block);

                // Merge block
                let result_type = self.typed_ast.get_expr_ty(&expr_id);
                func.body.switch_to_block(merge_block);
                let phi = func
                    .body
                    .append_block_param(merge_block, to_type(result_type));
                func.body.seal_block(merge_block);
                phi
            }
        }
    }

    fn gen_stmt(&mut self, func: &mut FuncCtx, stmt_id: Id<Stmt>) {
        let stmt = self.typed_ast.ast.stmts.get(&stmt_id);

        match stmt {
            Stmt::Let { name, value, .. } => {
                let val = self.gen_expr(func, *value);
                let var = func
                    .body
                    .declare_var(to_type(self.typed_ast.get_expr_ty(value)));
                func.body.def_var(var, val);
                func.env.insert(name.value.clone(), var);
            }
            Stmt::Expr { expr } => {
                let _ = self.gen_expr(func, *expr);
            }
        }

        // todo
    }

    fn build_signature(&self, func: &Func) -> Signature {
        let mut sig = Signature::new(self.isa.default_call_conv());

        sig.params = func
            .parameters
            .iter()
            .filter_map(|p| p.ty.as_ref().map(|ty| AbiParam::new(to_type(ty))))
            .collect();

        if let Some(ret_ty) = &func.return_type {
            sig.returns = vec![AbiParam::new(to_type(ret_ty))];
        }

        sig
    }

    fn build_extern_signature(&self, func: &ExternFunc) -> Signature {
        let mut sig = Signature::new(self.isa.default_call_conv());

        sig.params = func
            .parameters
            .iter()
            .filter_map(|p| p.ty.as_ref().map(|ty| AbiParam::new(to_type(ty))))
            .collect();

        if let Some(ret_ty) = &func.return_type {
            sig.returns = vec![AbiParam::new(to_type(ret_ty))];
        }

        sig
    }
}

pub fn to_type(ty: &Type) -> cranelift::prelude::Type {
    const POINTER: cranelift::prelude::Type = types::I64;

    match ty {
        Type::Unit => types::I8,
        Type::Unknown(..) => unreachable!("inferred during type inference"),
        Type::Named(..) => POINTER,
        Type::I32 => types::I32,
        Type::Bool => types::I8,
        Type::Str => POINTER,
        Type::Reference { .. } => POINTER,
        Type::Fun { .. } => POINTER,
    }
}

struct FuncCtx<'a> {
    body: FunctionBuilder<'a>,
    env: ScopedEnvironment<Variable>,
}

impl<'a> FuncCtx<'a> {
    fn new(func: &'a mut Function, func_builder_ctx: &'a mut FunctionBuilderContext) -> Self {
        Self {
            body: FunctionBuilder::new(func, func_builder_ctx),
            env: ScopedEnvironment::new(),
        }
    }

    pub fn finalize(self) {
        self.body.finalize();
    }
}

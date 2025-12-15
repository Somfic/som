use std::{collections::HashMap, sync::Arc};

use cranelift::{
    codegen::ir::Function,
    module::*,
    object::*,
    prelude::{settings::Flags, *},
};
use target_lexicon::Triple;

use crate::{BinOp, Diagnostic, Expr, ExprId, Func, Stmt, StmtId, Type, TypedAst};

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
        })
    }

    pub fn compile(mut self) -> Result<ObjectProduct> {
        let main_function = self
            .typed_ast
            .ast
            .funcs
            .iter()
            .find(|f| f.name.value == "main".into())
            .ok_or_else(|| CodegenError::NoMainFunction.to_diagnostic())?;

        self.gen_func(main_function)?;

        Ok(self.module.finish())
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

    fn gen_expr(&self, func: &mut FuncCtx, expr_id: ExprId) -> Value {
        let expr = self.typed_ast.ast.get_expr(&expr_id);

        match expr {
            Expr::Hole => unreachable!("parser should have thrown an error"),
            Expr::I32(v) => func.body.ins().iconst(to_type(&Type::I32), *v as i64),
            Expr::Bool(v) => func
                .body
                .ins()
                .iconst(to_type(&Type::Bool), if *v { 1 } else { 0 }),
            Expr::String(_) => todo!(),
            Expr::Var(ident) => func
                .body
                .use_var(*func.variables.get(&*ident.value).unwrap()),
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
                    BinOp::Equals => func.body.ins().icmp(IntCC::Equal, lhs_val, rhs_val),
                    BinOp::NotEquals => func.body.ins().icmp(IntCC::NotEqual, lhs_val, rhs_val),
                    // Boolean ops
                    BinOp::And => func.body.ins().band(lhs_val, rhs_val),
                    BinOp::Or => func.body.ins().bor(lhs_val, rhs_val),
                }
            }
            Expr::Block { stmts, value } => {
                for stmt in stmts {
                    self.gen_stmt(func, *stmt);
                }

                match value {
                    Some(expr_id) => self.gen_expr(func, *expr_id),
                    None => todo!(),
                }
            }
            Expr::Call { func, args } => todo!(),
            Expr::Borrow { mutable, expr } => todo!(),
            Expr::Deref { expr } => todo!(),
        }
    }

    fn gen_stmt(&self, func: &mut FuncCtx, stmt_id: StmtId) {
        let stmt = self.typed_ast.ast.get_stmt(&stmt_id);

        match stmt {
            Stmt::Let { name, value, .. } => {
                let val = self.gen_expr(func, *value);
                let var = func
                    .body
                    .declare_var(to_type(self.typed_ast.get_expr_ty(value)));
                func.body.def_var(var, val);
                func.variables.insert(name.value.to_string(), var);
            }
            Stmt::Expr { expr } => {
                let _ = self.gen_expr(func, *expr);
            }
        }

        // todo
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
    variables: HashMap<String, Variable>,
}

impl<'a> FuncCtx<'a> {
    fn new(func: &'a mut Function, func_builder_ctx: &'a mut FunctionBuilderContext) -> Self {
        Self {
            body: FunctionBuilder::new(func, func_builder_ctx),
            variables: HashMap::new(),
        }
    }

    pub fn finalize(self) {
        self.body.finalize();
    }
}

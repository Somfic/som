mod error;

use crate::{
    BinOp, Diagnostic, Expr, Func, Stmt, Type, TypedAst, arena::Id, scope::ScopedEnvironment,
};
use cranelift::{
    codegen::ir::{Function, layout},
    module::*,
    object::*,
    prelude::{settings::Flags, *},
};
use std::collections::HashMap;
use std::sync::Arc;
use target_lexicon::Triple;

use error::CodegenError;

type Result<T> = std::result::Result<T, Diagnostic>;

pub struct Codegen<'ast> {
    isa: Arc<dyn isa::TargetIsa>,
    typed_ast: &'ast TypedAst,
    module: ObjectModule,
    /// Unified map of all callable functions by name
    func_ids: HashMap<String, cranelift::module::FuncId>,
    /// Current module being compiled (for unqualified function lookups)
    current_module: Option<String>,
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
            func_ids: HashMap::new(),
            current_module: None,
        })
    }

    pub fn compile(mut self) -> Result<ObjectProduct> {
        // Declare all functions using the unified registry
        for (name, entry) in &self.typed_ast.ast.func_registry {
            let linkage = match &entry.kind {
                crate::FuncKind::Regular(_) => {
                    if name == "main" || name.ends_with("::main") {
                        Linkage::Export
                    } else {
                        Linkage::Local
                    }
                }
                crate::FuncKind::Extern(_) => Linkage::Import,
            };

            // For extern functions, use just the function name (not qualified)
            // For main, export as "main"
            // For other functions, use the full qualified name
            let symbol_name: &str = if name == "main" || name.ends_with("::main") {
                "main"
            } else if matches!(entry.kind, crate::FuncKind::Extern(_)) {
                // Extract just the function name for extern imports
                name.rsplit("::").next().unwrap_or(name)
            } else {
                name
            };

            let sig = self.build_signature_from_entry(entry);
            let func_id = self
                .module
                .declare_function(symbol_name, linkage, &sig)
                .map_err(|e| {
                    CodegenError::ModuleError(format!("failed to declare function {}: {}", name, e))
                        .to_diagnostic()
                })?;

            self.func_ids.insert(name.clone(), func_id);
        }

        // Generate bodies for regular functions only
        for (name, entry) in &self.typed_ast.ast.func_registry {
            if let crate::FuncKind::Regular(func_id) = &entry.kind {
                let func = self.typed_ast.ast.funcs.get(func_id);
                self.gen_func(func, name)?;
            }
        }

        Ok(self.module.finish())
    }

    fn gen_func(&mut self, func: &Func, name: &str) -> Result<()> {
        // Extract module name from qualified function name (e.g., "std::println" -> "std")
        // For methods like "std::String::len", we want "std" not "std::String"
        self.current_module = if let Some(pos) = name.find("::") {
            Some(name[..pos].to_string())
        } else {
            None
        };

        let return_type = func.return_type.as_ref().unwrap();

        let mut ctx = self.module.make_context();

        // function signature
        let mut sig = Signature::new(self.isa.default_call_conv());

        // Generate params, expanding structs larger than 8 bytes into multiple registers
        for param in &func.parameters {
            if let Some(ty) = &param.ty {
                if let Type::Named(struct_name) = ty {
                    let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                    let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                    let layout = struct_def.compute_layout();

                    if layout.size <= 8 {
                        sig.params.push(AbiParam::new(types::I64));
                    } else if layout.size <= 16 {
                        sig.params.push(AbiParam::new(types::I64));
                        sig.params.push(AbiParam::new(types::I64));
                    } else {
                        panic!("structs larger than 16 bytes not yet supported");
                    }
                } else {
                    sig.params.push(AbiParam::new(to_type(ty)));
                }
            }
        }

        // For struct returns, determine how many registers we need
        if let Type::Named(struct_name) = return_type {
            let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
            let struct_def = self.typed_ast.ast.structs.get(&struct_id);
            let layout = struct_def.compute_layout();

            if layout.size <= 8 {
                sig.returns = vec![AbiParam::new(types::I64)];
            } else if layout.size <= 16 {
                sig.returns = vec![AbiParam::new(types::I64), AbiParam::new(types::I64)];
            } else {
                panic!("structs larger than 16 bytes not yet supported");
            }
        } else {
            sig.returns = vec![AbiParam::new(to_type(return_type))];
        }
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
        let mut param_idx = 0;
        for param in func.parameters.iter() {
            if let Some(ty) = &param.ty {
                // For struct parameters, spill from register(s) to stack
                let final_value = if let Type::Named(struct_name) = ty {
                    let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                    let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                    let layout = struct_def.compute_layout();

                    // Create stack slot and store the register value(s)
                    let slot = func_ctx.body.create_sized_stack_slot(StackSlotData::new(
                        StackSlotKind::ExplicitSlot,
                        layout.size as u32,
                        layout.alignment,
                    ));
                    let base = func_ctx.body.ins().stack_addr(types::I64, slot, 0);

                    if layout.size <= 8 {
                        func_ctx.body.ins().store(
                            MemFlags::new(),
                            block_params[param_idx],
                            base,
                            0,
                        );
                        param_idx += 1;
                    } else if layout.size <= 16 {
                        func_ctx.body.ins().store(
                            MemFlags::new(),
                            block_params[param_idx],
                            base,
                            0,
                        );
                        func_ctx.body.ins().store(
                            MemFlags::new(),
                            block_params[param_idx + 1],
                            base,
                            8,
                        );
                        param_idx += 2;
                    } else {
                        panic!("structs larger than 16 bytes not yet supported");
                    }

                    base // Return stack address as the "value"
                } else {
                    let val = block_params[param_idx];
                    param_idx += 1;
                    val
                };

                let var = func_ctx.body.declare_var(to_type(ty));
                func_ctx.body.def_var(var, final_value);
                func_ctx.env.insert(param.name.to_string(), var);
            }
        }

        let return_value = self.gen_expr(&mut func_ctx, func.body);

        // For struct returns, load the struct from stack into register(s)
        if let Type::Named(struct_name) = return_type {
            let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
            let struct_def = self.typed_ast.ast.structs.get(&struct_id);
            let layout = struct_def.compute_layout();

            // return_value is a pointer to the struct on stack
            // Load the packed struct value to return in register(s)
            if layout.size <= 8 {
                let val = func_ctx
                    .body
                    .ins()
                    .load(types::I64, MemFlags::new(), return_value, 0);
                func_ctx.body.ins().return_(&[val]);
            } else if layout.size <= 16 {
                let val1 = func_ctx
                    .body
                    .ins()
                    .load(types::I64, MemFlags::new(), return_value, 0);
                let val2 = func_ctx
                    .body
                    .ins()
                    .load(types::I64, MemFlags::new(), return_value, 8);
                func_ctx.body.ins().return_(&[val1, val2]);
            } else {
                panic!("structs larger than 16 bytes not yet supported");
            }
        } else {
            func_ctx.body.ins().return_(&[return_value]);
        }

        // Print Cranelift IR
        println!("{}", ctx.func);

        // Use the already-declared func_id from compile()
        let func_id = *self
            .func_ids
            .get(name)
            .expect("function should be declared");
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
            Expr::I32(v) => {
                // Use the inferred type (could be i32, u8, etc.)
                let ty = self.typed_ast.get_expr_ty(&expr_id);
                func.body.ins().iconst(to_type(ty), *v as i64)
            }
            Expr::F32(v) => func.body.ins().f32const(*v),
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
                let var = func
                    .env
                    .get(&ident.name().to_string())
                    .expect("variable not found");

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
                    BinOp::And => func.body.ins().band(lhs_val, rhs_val),
                    BinOp::Or => func.body.ins().bor(lhs_val, rhs_val),
                    BinOp::Modulo => func.body.ins().srem(lhs_val, rhs_val),
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
                // Look up in unified func_ids map, trying qualified name if unqualified fails
                let registry_key = name.to_string();
                let callee_func_id = *self
                    .func_ids
                    .get(&registry_key)
                    .or_else(|| {
                        // Try with current module prefix
                        if let Some(module) = &self.current_module {
                            let qualified = format!("{}::{}", module, registry_key);
                            if let Some(id) = self.func_ids.get(&qualified) {
                                return Some(id);
                            }
                            // Try imported modules
                            for imported in self.typed_ast.ast.use_modules(module) {
                                let qualified = format!("{}::{}", imported, registry_key);
                                if let Some(id) = self.func_ids.get(&qualified) {
                                    return Some(id);
                                }
                            }
                        }
                        None
                    })
                    .expect("type checker should have caught unknown function");

                let callee_func_ref = self
                    .module
                    .declare_func_in_func(callee_func_id, func.body.func);

                // Generate arguments, handling struct types specially
                let mut arguments: Vec<Value> = Vec::new();
                for arg in args.iter() {
                    let val = self.gen_expr(func, *arg);
                    let arg_ty = self.typed_ast.get_expr_ty(arg);

                    // For struct arguments, we have a pointer but need to pass by value
                    // Load the packed struct value from the pointer
                    if let Type::Named(struct_name) = arg_ty {
                        let struct_id =
                            self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                        let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                        let layout = struct_def.compute_layout();

                        if layout.size <= 8 {
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                0,
                            ));
                        } else if layout.size <= 16 {
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                0,
                            ));
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                8,
                            ));
                        } else {
                            panic!("structs larger than 16 bytes not yet supported");
                        }
                    } else {
                        arguments.push(val);
                    }
                }

                let call = func.body.ins().call(callee_func_ref, &arguments);

                // Copy results to break the borrow on func.body
                let results: Vec<Value> = func.body.inst_results(call).to_vec();
                if results.is_empty() {
                    // Void function - return dummy value (unit type)
                    func.body.ins().iconst(types::I32, 0)
                } else {
                    // Check if return type is a struct
                    let return_ty = self.typed_ast.get_expr_ty(&expr_id);
                    if let Type::Named(struct_name) = return_ty {
                        // Struct returned in register(s) - spill to stack
                        let struct_id =
                            self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                        let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                        let layout = struct_def.compute_layout();

                        // Create stack slot for the struct
                        let slot = func.body.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            layout.size as u32,
                            layout.alignment,
                        ));

                        let base = func.body.ins().stack_addr(types::I64, slot, 0);

                        // Store the returned value(s) to the stack
                        // For structs ≤8 bytes: one i64 register
                        // For structs 9-16 bytes: two i64 registers
                        func.body.ins().store(MemFlags::new(), results[0], base, 0);
                        if results.len() > 1 {
                            func.body.ins().store(MemFlags::new(), results[1], base, 8);
                        }

                        base
                    } else {
                        results[0]
                    }
                }
            }
            Expr::Borrow { mutable: _, expr } => {
                // Get the type of the expression being borrowed
                let inner_ty = self.typed_ast.get_expr_ty(expr);

                match inner_ty {
                    // For struct types, the value is already a pointer to stack memory
                    Type::Named(_) => self.gen_expr(func, *expr),
                    // For primitives, spill to stack and return address
                    _ => {
                        let val = self.gen_expr(func, *expr);
                        let ty = to_type(inner_ty);
                        let slot = func.body.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            ty.bytes(),
                            ty.bytes() as u8,
                        ));
                        let addr = func.body.ins().stack_addr(types::I64, slot, 0);
                        func.body.ins().store(MemFlags::new(), val, addr, 0);
                        addr
                    }
                }
            }
            Expr::Deref { expr } => {
                // Get the pointer value
                let ptr = self.gen_expr(func, *expr);
                // Get the type we're dereferencing to
                let result_ty = self.typed_ast.get_expr_ty(&expr_id);
                // Load the value from the pointer
                func.body
                    .ins()
                    .load(to_type(result_ty), MemFlags::new(), ptr, 0)
            }
            Expr::Not { expr } => {
                let val = self.gen_expr(func, *expr);
                // Boolean NOT: XOR with 1
                let one = func.body.ins().iconst(types::I8, 1);
                func.body.ins().bxor(val, one)
            }
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
            Expr::Constructor {
                struct_name,
                fields,
            } => {
                let struct_id = self
                    .typed_ast
                    .ast
                    .find_struct_by_name(&struct_name.name().to_string())
                    .unwrap();

                let struct_type = self.typed_ast.ast.structs.get(&struct_id);
                let layout = struct_type.compute_layout();

                let slot = func.body.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    layout.size as u32,
                    layout.alignment,
                ));

                let base = func.body.ins().stack_addr(types::I64, slot, 0);

                for (i, (_, expr_id)) in fields.iter().enumerate() {
                    let value = self.gen_expr(func, *expr_id);
                    let offset = *layout.field_offsets.get(i).unwrap() as i32;
                    func.body.ins().store(MemFlags::new(), value, base, offset);
                }

                base
            }
            Expr::FieldAccess { object, field } => {
                // Get the base address of the struct
                let base = self.gen_expr(func, *object);

                // Look up the struct type from the object's type
                let obj_ty = self.typed_ast.get_expr_ty(object);
                let struct_name = match obj_ty {
                    Type::Named(name) => name,
                    Type::Reference { to, .. } => match to.as_ref() {
                        Type::Named(name) => name,
                        _ => panic!("field access on non-struct type"),
                    },
                    _ => panic!("field access on non-struct type"),
                };

                let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();

                let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                let layout = struct_def.compute_layout();

                // Find the field index and its type
                let field_idx = struct_def
                    .fields
                    .iter()
                    .position(|f| f.name.value == field.value)
                    .expect("field not found");

                let field_ty = &struct_def.fields[field_idx].ty;
                let offset = layout.field_offsets[field_idx] as i32;

                // Load the field value from memory
                func.body
                    .ins()
                    .load(to_type(field_ty), MemFlags::new(), base, offset)
            }
            Expr::MethodCall {
                object,
                method,
                args,
            } => {
                // Determine struct name from the object's type
                let obj_ty = self.typed_ast.get_expr_ty(object);
                let struct_name = match obj_ty {
                    Type::Named(name) => name.as_ref(),
                    Type::Reference { to, .. } => match to.as_ref() {
                        Type::Named(name) => name.as_ref(),
                        _ => panic!("method call on non-struct type"),
                    },
                    _ => panic!("method call on non-struct type"),
                };

                // Look up StructName::method in func_ids
                let method_key = format!("{}::{}", struct_name, method.value);
                let callee_func_id = *self
                    .func_ids
                    .get(&method_key)
                    .or_else(|| {
                        if let Some(module) = &self.current_module {
                            let qualified = format!("{}::{}", module, method_key);
                            if let Some(id) = self.func_ids.get(&qualified) {
                                return Some(id);
                            }
                            for imported in self.typed_ast.ast.use_modules(module) {
                                let qualified = format!("{}::{}", imported, method_key);
                                if let Some(id) = self.func_ids.get(&qualified) {
                                    return Some(id);
                                }
                            }
                        }
                        None
                    })
                    .expect("type checker should have caught unknown method");

                let callee_func_ref = self
                    .module
                    .declare_func_in_func(callee_func_id, func.body.func);

                // Generate arguments: object (self) first, then remaining args
                let mut arguments: Vec<Value> = Vec::new();

                // Generate self argument (the object)
                let self_val = self.gen_expr(func, *object);
                if let Type::Named(sn) = obj_ty {
                    let struct_id = self.typed_ast.ast.find_struct_by_name(sn).unwrap();
                    let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                    let layout = struct_def.compute_layout();
                    if layout.size <= 8 {
                        arguments.push(func.body.ins().load(
                            types::I64,
                            MemFlags::new(),
                            self_val,
                            0,
                        ));
                    } else if layout.size <= 16 {
                        arguments.push(func.body.ins().load(
                            types::I64,
                            MemFlags::new(),
                            self_val,
                            0,
                        ));
                        arguments.push(func.body.ins().load(
                            types::I64,
                            MemFlags::new(),
                            self_val,
                            8,
                        ));
                    } else {
                        panic!("structs larger than 16 bytes not yet supported");
                    }
                } else {
                    arguments.push(self_val);
                }

                // Generate remaining arguments
                for arg in args.iter() {
                    let val = self.gen_expr(func, *arg);
                    let arg_ty = self.typed_ast.get_expr_ty(arg);

                    if let Type::Named(sn) = arg_ty {
                        let struct_id = self.typed_ast.ast.find_struct_by_name(sn).unwrap();
                        let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                        let layout = struct_def.compute_layout();
                        if layout.size <= 8 {
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                0,
                            ));
                        } else if layout.size <= 16 {
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                0,
                            ));
                            arguments.push(func.body.ins().load(
                                types::I64,
                                MemFlags::new(),
                                val,
                                8,
                            ));
                        } else {
                            panic!("structs larger than 16 bytes not yet supported");
                        }
                    } else {
                        arguments.push(val);
                    }
                }

                let call = func.body.ins().call(callee_func_ref, &arguments);
                let results: Vec<Value> = func.body.inst_results(call).to_vec();

                if results.is_empty() {
                    func.body.ins().iconst(types::I32, 0)
                } else {
                    let return_ty = self.typed_ast.get_expr_ty(&expr_id);
                    if let Type::Named(sn) = return_ty {
                        let struct_id = self.typed_ast.ast.find_struct_by_name(sn).unwrap();
                        let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                        let layout = struct_def.compute_layout();
                        let slot = func.body.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            layout.size as u32,
                            layout.alignment,
                        ));
                        let base = func.body.ins().stack_addr(types::I64, slot, 0);
                        func.body.ins().store(MemFlags::new(), results[0], base, 0);
                        if results.len() > 1 {
                            func.body.ins().store(MemFlags::new(), results[1], base, 8);
                        }
                        base
                    } else {
                        results[0]
                    }
                }
            }
            Expr::Assignment { target, value } => {
                // Generate the value to be assigned
                let value_val = self.gen_expr(func, *value);

                // Look up the target variable
                let target_expr = self.typed_ast.ast.exprs.get(target);
                let var_name = match target_expr {
                    Expr::Var(ident) => ident.name().to_string(),
                    _ => panic!("assignment target must be a variable"),
                };

                let var = func
                    .env
                    .get(&var_name)
                    .expect("assignment target variable not found");

                // Store the value into the variable
                func.body.def_var(*var, value_val);

                value_val
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
            Stmt::Loop { body } => {
                // Create loop header block
                let loop_block = func.body.create_block();

                // Jump into loop
                func.body.ins().jump(loop_block, &[]);

                // Switch to loop block
                func.body.switch_to_block(loop_block);

                // Generate body statements
                for stmt in body {
                    self.gen_stmt(func, *stmt);
                }

                // Jump back to loop header (infinite loop)
                func.body.ins().jump(loop_block, &[]);
                func.body.seal_block(loop_block);

                // Create exit block (unreachable without break, but needed for well-formed IR)
                let exit_block = func.body.create_block();
                func.body.switch_to_block(exit_block);
                func.body.seal_block(exit_block);
            }
            Stmt::While { condition, body } => {
                // Create blocks for while loop
                let header_block = func.body.create_block();
                let body_block = func.body.create_block();
                let exit_block = func.body.create_block();

                // Jump to header
                func.body.ins().jump(header_block, &[]);

                // Header block: evaluate condition and branch
                func.body.switch_to_block(header_block);
                let condition_val = self.gen_expr(func, *condition);
                func.body
                    .ins()
                    .brif(condition_val, body_block, &[], exit_block, &[]);

                // Body block: execute statements and jump back to header
                func.body.switch_to_block(body_block);
                func.body.seal_block(body_block);
                for stmt in body {
                    self.gen_stmt(func, *stmt);
                }
                func.body.ins().jump(header_block, &[]);

                // Seal header after body (since body jumps back to it)
                func.body.seal_block(header_block);

                // Exit block: continue after loop
                func.body.switch_to_block(exit_block);
                func.body.seal_block(exit_block);
            }
            Stmt::Condition {
                condition,
                then_body,
                else_body,
            } => {
                let then_block = func.body.create_block();
                let else_block = func.body.create_block();
                let merge_block = func.body.create_block();

                // Evaluate condition and branch
                let condition_val = self.gen_expr(func, *condition);
                func.body
                    .ins()
                    .brif(condition_val, then_block, &[], else_block, &[]);

                // Then block
                func.body.switch_to_block(then_block);
                func.body.seal_block(then_block);
                for stmt in then_body {
                    self.gen_stmt(func, *stmt);
                }
                func.body.ins().jump(merge_block, &[]);

                // Else block
                func.body.switch_to_block(else_block);
                func.body.seal_block(else_block);
                if let Some(else_stmts) = else_body {
                    for stmt in else_stmts {
                        self.gen_stmt(func, *stmt);
                    }
                }
                func.body.ins().jump(merge_block, &[]);

                // Merge block
                func.body.switch_to_block(merge_block);
                func.body.seal_block(merge_block);
            }
        }
    }

    fn build_signature_from_entry(&self, entry: &crate::FuncEntry) -> Signature {
        let mut sig = Signature::new(self.isa.default_call_conv());

        // Generate params, expanding structs larger than 8 bytes into multiple registers
        for ty in &entry.signature.params {
            if let Type::Named(struct_name) = ty {
                let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                let layout = struct_def.compute_layout();

                if layout.size <= 8 {
                    sig.params.push(AbiParam::new(types::I64));
                } else if layout.size <= 16 {
                    sig.params.push(AbiParam::new(types::I64));
                    sig.params.push(AbiParam::new(types::I64));
                } else {
                    panic!("structs larger than 16 bytes not yet supported");
                }
            } else {
                sig.params.push(AbiParam::new(to_type(ty)));
            }
        }

        if entry.signature.return_type != Type::Unit {
            // For struct returns, determine how many registers we need
            if let Type::Named(struct_name) = &entry.signature.return_type {
                let struct_id = self.typed_ast.ast.find_struct_by_name(struct_name).unwrap();
                let struct_def = self.typed_ast.ast.structs.get(&struct_id);
                let layout = struct_def.compute_layout();

                if layout.size <= 8 {
                    sig.returns = vec![AbiParam::new(types::I64)];
                } else if layout.size <= 16 {
                    sig.returns = vec![AbiParam::new(types::I64), AbiParam::new(types::I64)];
                } else {
                    panic!("structs larger than 16 bytes not yet supported");
                }
            } else {
                sig.returns = vec![AbiParam::new(to_type(&entry.signature.return_type))];
            }
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
        Type::U8 => types::I8,
        Type::F32 => types::F32,
        Type::Bool => types::I8,
        Type::Str => POINTER,
        Type::Reference { .. } => POINTER,
        Type::Fun { .. } => POINTER,
        Type::Pointer => POINTER,
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

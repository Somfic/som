use cranelift::{
    codegen::ir::BlockArg,
    module::{FuncId, Linkage, Module, ModuleError},
    prelude::{
        types, AbiParam, FunctionBuilder, InstBuilder, IntCC, MemFlags, Signature, StackSlotData,
        StackSlotKind, Value,
    },
};

use crate::{
    ast::{
        Assignment, Binary, BinaryOperation, Block, Call, Construction, Expression, FieldAccess,
        Group, I64Type, Lambda, Primary, PrimaryKind, StructField, StructType, Ternary, Type,
        Unary, UnaryOperation,
    },
    emit::FunctionRegistry,
    lexer::Identifier,
    Emit, EmitError, FunctionContext, ModuleContext, Result, Typed,
};

impl Emit for Expression<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        match self {
            Expression::Primary(primary) => primary.declare(ctx),
            Expression::Unary(unary) => unary.declare(ctx),
            Expression::Binary(binary) => binary.declare(ctx),
            Expression::Group(group) => group.declare(ctx),
            Expression::Block(block) => block.declare(ctx),
            Expression::Ternary(ternary) => ternary.declare(ctx),
            Expression::Lambda(lambda) => lambda.declare(ctx),
            Expression::Call(call) => call.declare(ctx),
            Expression::Construction(construction) => construction.declare(ctx),
            Expression::FieldAccess(field_access) => field_access.declare(ctx),
            Expression::Assignment(assignment) => assignment.declare(ctx),
        }
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        match self {
            Expression::Primary(p) => p.emit(ctx),
            Expression::Unary(u) => u.emit(ctx),
            Expression::Binary(b) => b.emit(ctx),
            Expression::Group(g) => g.emit(ctx),
            Expression::Block(b) => b.emit(ctx),
            Expression::Ternary(t) => t.emit(ctx),
            Expression::Lambda(l) => l.emit(ctx),
            Expression::Call(c) => c.emit(ctx),
            Expression::Construction(construction) => construction.emit(ctx),
            Expression::FieldAccess(field_access) => field_access.emit(ctx),
            Expression::Assignment(assignment) => assignment.emit(ctx),
        }
    }
}

impl Emit for Primary<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        Ok(match &self.kind {
            PrimaryKind::Boolean(b) => ctx.builder.ins().iconst(types::I8, if *b { 1 } else { 0 }),
            PrimaryKind::I32(i) => ctx.builder.ins().iconst(types::I32, *i as i64),
            PrimaryKind::I64(i) => ctx.builder.ins().iconst(types::I64, *i),
            PrimaryKind::Decimal(d) => ctx.builder.ins().f64const(*d),
            PrimaryKind::String(s) => {
                // Allocate space for string + null terminator
                let len = s.len() as u32;
                let string_bytes = s.as_bytes();
                let stack_slot = ctx.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    len + 1, // +1 for null terminator
                    0,
                ));
                let pointer = ctx.builder.ins().stack_addr(types::I64, stack_slot, 0);

                // Store string bytes
                for (i, &byte) in string_bytes.iter().enumerate() {
                    let byte = ctx.builder.ins().iconst(types::I8, byte as i64);
                    ctx.builder
                        .ins()
                        .store(MemFlags::new(), byte, pointer, i as i32);
                }

                // Add null terminator
                let null_byte = ctx.builder.ins().iconst(types::I8, 0);
                ctx.builder
                    .ins()
                    .store(MemFlags::new(), null_byte, pointer, len as i32);

                // Create a struct containing ptr and len
                let struct_size = 16; // 8 bytes for ptr + 8 bytes for len
                let struct_slot = ctx.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    struct_size,
                    0,
                ));
                let struct_pointer = ctx.builder.ins().stack_addr(types::I64, struct_slot, 0);

                // Store ptr at offset 0
                ctx.builder
                    .ins()
                    .store(MemFlags::new(), pointer, struct_pointer, 0);

                // Store len at offset 8
                let len_value = ctx.builder.ins().iconst(types::I64, len as i64);
                ctx.builder
                    .ins()
                    .store(MemFlags::new(), len_value, struct_pointer, 8);

                struct_pointer
            }
            PrimaryKind::Character(c) => unimplemented!("character emit"),
            PrimaryKind::Identifier(ident) => ident.emit(ctx)?,
        })
    }
}

impl Emit for Unary<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.value.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let value = self.value.emit(ctx)?;

        match &self.op {
            UnaryOperation::Negate => Ok(ctx.builder.ins().ineg(value)),
        }
    }
}

impl Emit for Binary<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.lhs.declare(ctx)?;
        self.rhs.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let lhs = self.lhs.emit(ctx)?;
        let rhs = self.rhs.emit(ctx)?;

        // TODO: support i32 vs i64 vs f64
        Ok(match &self.op {
            BinaryOperation::Add => ctx.builder.ins().iadd(lhs, rhs),
            BinaryOperation::Subtract => ctx.builder.ins().isub(lhs, rhs),
            BinaryOperation::Multiply => ctx.builder.ins().imul(lhs, rhs),
            BinaryOperation::Divide => ctx.builder.ins().fdiv(lhs, rhs),
            BinaryOperation::LessThan => ctx.builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs),
            BinaryOperation::LessThanOrEqual => {
                ctx.builder
                    .ins()
                    .icmp(IntCC::SignedLessThanOrEqual, lhs, rhs)
            }
            BinaryOperation::GreaterThan => {
                ctx.builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs)
            }
            BinaryOperation::GreaterThanOrEqual => {
                ctx.builder
                    .ins()
                    .icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs)
            }
            BinaryOperation::Equality => ctx.builder.ins().icmp(IntCC::Equal, lhs, rhs),
            BinaryOperation::Inequality => ctx.builder.ins().icmp(IntCC::NotEqual, lhs, rhs),
        })
    }
}

impl Emit for Group<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.expr.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        self.expr.emit(ctx)
    }
}

impl Emit for Block<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        for statement in &self.statements {
            statement.declare(ctx)?;
        }

        if let Some(expression) = &self.expression {
            expression.declare(ctx)?;
        }

        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        for statement in &self.statements {
            statement.emit(ctx)?;
        }

        match &self.expression {
            Some(expression) => expression.emit(ctx),
            None => Ok(ctx.builder.ins().iconst(types::I8, 0)),
        }
    }
}

impl Emit for Ternary<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.condition.declare(ctx)?;
        self.truthy.declare(ctx)?;
        self.falsy.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let condition = self.condition.emit(ctx)?;

        let truthy_block = ctx.builder.create_block();
        let falsy_block = ctx.builder.create_block();
        let merge_block = ctx.builder.create_block();

        ctx.builder
            .append_block_param(merge_block, self.truthy.ty().clone().into());

        ctx.builder
            .ins()
            .brif(condition, truthy_block, &[], falsy_block, &[]);

        // truthy
        ctx.builder.switch_to_block(truthy_block);
        let value = self.truthy.emit(ctx)?;
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(value)]);

        // falsy
        ctx.builder.switch_to_block(falsy_block);
        let value = self.falsy.emit(ctx)?;
        ctx.builder
            .ins()
            .jump(merge_block, &[BlockArg::Value(value)]);

        // merge
        ctx.builder.switch_to_block(merge_block);
        let value = ctx.builder.block_params(merge_block)[0];

        Ok(value)
    }
}

impl Emit for Lambda<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.body.declare(ctx)?;

        let mut sig = Signature::new(ctx.isa.default_call_conv());
        for param in &self.parameters {
            sig.params.push(AbiParam::new(param.ty.clone().into()));
        }
        if let Type::Function(f) = &self.ty {
            sig.returns.push(AbiParam::new((*f.returns).clone().into()));
        }

        let func_id = ctx
            .module
            .declare_function(&format!("lambda_{}", self.id), Linkage::Local, &sig)
            .map_err(|e| EmitError::ModuleError(e).to_diagnostic())?;

        ctx.function_registry.register(self.id, func_id, sig);

        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let (func_id, sig) = ctx
            .function_registry
            .get(self.id)
            .ok_or_else(|| EmitError::UndefinedFunction.to_diagnostic())?
            .clone();

        self.compile_body(
            ctx.module,
            ctx.function_registry,
            func_id,
            sig,
            None,
            ctx.global_functions,
        )?;

        let reference = ctx.module.declare_func_in_func(func_id, ctx.builder.func);
        let address = ctx.builder.ins().func_addr(types::I64, reference);

        Ok(address)
    }
}

impl Lambda<Typed> {
    pub fn compile_body(
        &self,
        module: &mut dyn Module,
        function_registry: &FunctionRegistry,
        func_id: FuncId,
        sig: Signature,
        self_name: Option<String>,
        global_functions: &std::collections::HashMap<String, usize>,
    ) -> Result<()> {
        use cranelift::codegen::ir::UserFuncName;
        use cranelift::prelude::FunctionBuilderContext;

        let mut cranelift_ctx = module.make_context();
        cranelift_ctx.func.signature = sig;
        cranelift_ctx.func.name = UserFuncName::user(0, func_id.as_u32());

        let mut builder_context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut cranelift_ctx.func, &mut builder_context);

        let entry_block = builder.create_block();

        for param in &self.parameters {
            builder.append_block_param(entry_block, param.ty.clone().into());
        }

        builder.switch_to_block(entry_block);

        let param_values: Vec<_> = (0..self.parameters.len())
            .map(|i| builder.block_params(entry_block)[i])
            .collect();

        let extern_registry = std::collections::HashMap::new();
        let mut func_ctx = FunctionContext::new(
            &mut builder,
            module,
            function_registry,
            &extern_registry,
            global_functions,
        );

        // if this lambda is self-referencing, register its name
        if let Some(name) = self_name {
            func_ctx.self_referencing_lambdas.insert(name, self.id);
        }

        for (param, &cranelift_param) in self.parameters.iter().zip(&param_values) {
            let var = func_ctx.declare_variable(param.name.name.to_string(), param.ty.clone());
            func_ctx.builder.def_var(var, cranelift_param);
        }

        let return_val = self.body.emit(&mut func_ctx)?;
        func_ctx.builder.ins().return_(&[return_val]);
        func_ctx.builder.seal_all_blocks();

        module
            .define_function(func_id, &mut cranelift_ctx)
            .map_err(|e| EmitError::ModuleError(e).to_diagnostic())?;

        module.clear_context(&mut cranelift_ctx);
        Ok(())
    }
}

impl Emit for Call<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.callee.declare(ctx)?;
        for arg in &self.arguments {
            arg.declare(ctx)?;
        }
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        // Check if this is a call to an extern function
        if let Expression::Primary(Primary {
            kind: PrimaryKind::Identifier(ident),
            ..
        }) = &*self.callee
        {
            if let Some((func_id, sig)) = ctx.extern_registry.get(&*ident.name) {
                // This is an extern function - use direct call
                let arguments: Vec<Value> = self
                    .arguments
                    .iter()
                    .zip(&sig.params)
                    .map(|(arg, param)| {
                        let value = arg.emit(ctx)?;

                        // Check if we need to convert String to *byte
                        if matches!(arg.ty(), Type::String(_)) && param.value_type == types::I64 {
                            // Extract pointer from string struct (pointer is at offset 0)
                            let pointer =
                                ctx.builder
                                    .ins()
                                    .load(types::I64, MemFlags::new(), value, 0);
                            Ok(pointer)
                        } else {
                            Ok(value)
                        }
                    })
                    .collect::<Result<_>>()?;

                let func_ref = ctx.module.declare_func_in_func(*func_id, ctx.builder.func);
                let call = ctx.builder.ins().call(func_ref, &arguments);
                let results = ctx.builder.inst_results(call);
                return Ok(results[0]);
            }
        }

        // Regular indirect call for lambdas
        let pointer = self.callee.emit(ctx)?;

        let arguments: Vec<Value> = self
            .arguments
            .iter()
            .map(|arg| arg.emit(ctx))
            .collect::<Result<_>>()?;

        let mut signature = Signature::new(ctx.builder.func.signature.call_conv);
        let (parameter_types, return_type) = match self.callee.ty() {
            Type::Function(f) => (&f.parameters, &f.returns),
            _ => unreachable!(),
        };

        for parameter_type in parameter_types {
            signature
                .params
                .push(AbiParam::new(parameter_type.clone().into()));
        }

        signature
            .returns
            .push(AbiParam::new((**return_type).clone().into()));

        let sig_ref = ctx.builder.import_signature(signature);

        let call = ctx
            .builder
            .ins()
            .call_indirect(sig_ref, pointer, &arguments);

        let results = ctx.builder.inst_results(call);

        Ok(results[0])
    }
}

impl Emit for Construction<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        for (_, expr) in &self.fields {
            expr.declare(ctx)?;
        }
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let mut offset = 0;
        let mut offsets = vec![];

        for (field_name, field_value) in &self.fields {
            let field_ty = field_value.ty();
            let field_offset = field_ty.size();
            offsets.push((offset, field_value));
            offset += field_offset;
        }

        let total_size = offset;

        let stack_slot = ctx.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            total_size,
            0,
        ));

        let pointer = ctx.builder.ins().stack_addr(types::I64, stack_slot, 0);

        for (offset, value) in offsets {
            let value = value.emit(ctx)?;

            ctx.builder
                .ins()
                .store(MemFlags::new(), value, pointer, offset as i32);
        }

        Ok(pointer)
    }
}

impl Emit for FieldAccess<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.object.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let pointer = self.object.emit(ctx)?;

        let Type::Struct(struct_type) = self.object.ty() else {
            unreachable!("type checker ensures this is a struct")
        };

        let mut offset = 0;
        let mut field_type = None;

        for field in &struct_type.fields {
            if field.name == self.field {
                field_type = Some(&field.ty);
                break; // Found it!
            }
            offset += field.ty.size(); // Accumulate offset
        }

        let field_type = field_type.expect("type checker ensures field exists");

        let cranelift_type = field_type.clone().into();
        let value = ctx
            .builder
            .ins()
            .load(cranelift_type, MemFlags::new(), pointer, offset as i32);

        Ok(value)
    }
}

impl Emit for Assignment<Typed> {
    type Output = Value;

    fn declare(&self, ctx: &mut ModuleContext) -> Result<()> {
        self.value.declare(ctx)?;
        Ok(())
    }

    fn emit(&self, ctx: &mut FunctionContext) -> Result<Self::Output> {
        let value = self.value.emit(ctx)?;

        let variable = match &*self.target {
            Expression::Primary(Primary {
                kind: PrimaryKind::Identifier(ident),
                ..
            }) => ident,
            _ => return Err(EmitError::InvalidAssignmentTarget.to_diagnostic()),
        };

        let variable = ctx
            .get_variable(&variable.name)
            .or_else(|_| EmitError::UndefinedVariable.to_diagnostic().to_err())?;

        ctx.builder.def_var(variable, value);

        Ok(value)
    }
}

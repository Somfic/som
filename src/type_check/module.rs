use crate::{
    ast::{
        Declaration, FunctionType, PointerType, StructField, StructType, Type, UnitType, Visibility,
    },
    lexer::Path,
    Module, Result, TypeCheck, TypeCheckContext, TypeCheckError, Typed, Untyped,
};
use std::collections::HashMap;

#[derive(Default)]
pub struct ModuleTyper {
    registry: HashMap<Path, ModuleScope>,
}

#[derive(Default)]
pub struct ModuleScope {
    // pub items
    pub public_types: HashMap<String, Type>,
    pub public_variables: HashMap<String, Type>,

    // pub mod items
    pub module_types: HashMap<String, Type>,
    pub module_variables: HashMap<String, Type>,
}

impl ModuleTyper {
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    pub fn check(&mut self, modules: Vec<Module<Untyped>>) -> Result<Vec<Module<Typed>>> {
        let modules_map: HashMap<Path, Module<Untyped>> =
            modules.into_iter().map(|m| (m.path.clone(), m)).collect();

        for (path, module) in &modules_map {
            self.declare(path, module);
        }

        for (path, module) in &modules_map {
            self.define(path, module)?;
        }

        let mut typed_modules = vec![];
        for module in modules_map.into_values() {
            let typed_module = self.check_module(module)?;
            typed_modules.push(typed_module);
        }

        Ok(typed_modules)
    }

    fn declare(&mut self, path: &Path, module: &Module<Untyped>) {
        let mut scope = ModuleScope::default();

        for file in &module.files {
            for declaration in &file.declarations {
                match declaration {
                    Declaration::TypeDefinition(type_definition) => {
                        let forward_type = Type::Forward(type_definition.name.clone());

                        match type_definition.visibility {
                            Visibility::Private => { /* not added to scope */ }
                            Visibility::Module => {
                                scope
                                    .module_types
                                    .insert(type_definition.name.to_string(), forward_type);
                            }
                            Visibility::Public => {
                                let name = type_definition.name.to_string();
                                scope
                                    .public_types
                                    .insert(name.clone(), forward_type.clone());
                                scope.module_types.insert(name, forward_type);
                            }
                        }
                    }
                    _ => { /* we only declare types */ }
                }
            }
        }

        self.registry.insert(path.clone(), scope);
    }

    fn define(&mut self, path: &Path, module: &Module<Untyped>) -> Result<()> {
        let scope = self
            .registry
            .get(path)
            .expect("to be filled in by previous pass");
        let mut ctx = TypeCheckContext::new();

        for (name, ty) in &scope.module_types {
            ctx.declare_type(name.clone(), ty.clone());
        }

        for file in &module.files {
            for declaration in &file.declarations {
                match declaration {
                    Declaration::TypeDefinition(type_definition) => {
                        if matches!(type_definition.visibility, Visibility::Private) {
                            continue;
                        }

                        let resolved_type = self.resolve_type(&type_definition.ty, &ctx)?;

                        let name = type_definition.name.to_string();
                        let scope = self
                            .registry
                            .get_mut(path)
                            .expect("to be filled in by previous pass");

                        match type_definition.visibility {
                            Visibility::Private => unreachable!(),
                            Visibility::Module => {
                                scope.module_types.insert(name, resolved_type.clone());
                            }
                            Visibility::Public => {
                                scope
                                    .public_types
                                    .insert(name.clone(), resolved_type.clone());
                                scope.module_types.insert(name, resolved_type.clone());
                            }
                        }

                        ctx.declare_type(type_definition.name.to_string(), resolved_type);
                    }
                    _ => { /* we only define types */ }
                }
            }
        }

        Ok(())
    }

    fn check_module(&mut self, module: Module<Untyped>) -> Result<Module<Typed>> {
        let scope = self
            .registry
            .get(&module.path)
            .expect("to be filled in by previous pass");

        let mut ctx = TypeCheckContext::new();

        for (name, ty) in &scope.module_types {
            ctx.declare_type(name.clone(), ty.clone());
        }

        for (name, ty) in &scope.module_variables {
            ctx.declare_variable(name.clone(), ty.clone());
        }

        let mut files = vec![];
        let path = module.path.clone();
        for file in module.files {
            let mut ctx = ctx.new_child_context();
            let typed_file = file.type_check(&mut ctx)?;

            files.push(typed_file);
        }

        Ok(Module { files, path })
    }

    fn resolve_type(&self, ty: &Type, ctx: &TypeCheckContext) -> Result<Type> {
        self.resolve_type_with_depth(ty, ctx, 0)
    }

    fn resolve_type_with_depth(
        &self,
        ty: &Type,
        ctx: &TypeCheckContext,
        depth: usize,
    ) -> Result<Type> {
        const MAX_DEPTH: usize = 100;

        if depth > MAX_DEPTH {
            return Err(TypeCheckError::RecursiveType
                .to_diagnostic()
                .with_hint("Type definition contains infinite recursion"));
        }

        match ty {
            Type::Forward(forward) => {
                let actual_type = ctx.get_type(forward.name.to_string())?;

                match &actual_type {
                    Type::Forward(_) => Ok(actual_type),
                    _ => self.resolve_type_with_depth(&actual_type, ctx, depth + 1),
                }
            }

            // Struct - resolve field types
            Type::Struct(s) => {
                let mut resolved_fields = Vec::new();
                for field in &s.fields {
                    let resolved_field_type =
                        self.resolve_type_with_depth(&field.ty, ctx, depth + 1)?;
                    resolved_fields.push(StructField {
                        name: field.name.clone(),
                        ty: resolved_field_type,
                    });
                }
                Ok(Type::Struct(StructType {
                    name: s.name.clone(),
                    fields: resolved_fields,
                    span: s.span.clone(),
                }))
            }

            // Function - resolve parameter and return types
            Type::Function(f) => {
                let resolved_params = f
                    .parameters
                    .iter()
                    .map(|param| self.resolve_type_with_depth(param, ctx, depth + 1))
                    .collect::<Result<Vec<_>>>()?;

                let resolved_return =
                    Box::new(self.resolve_type_with_depth(&f.returns, ctx, depth + 1)?);

                Ok(Type::Function(FunctionType {
                    parameters: resolved_params,
                    returns: resolved_return,
                    span: f.span.clone(),
                }))
            }

            // Pointer - resolve pointee type
            Type::Pointer(p) => {
                let resolved_pointee =
                    Box::new(self.resolve_type_with_depth(&p.pointee, ctx, depth + 1)?);
                Ok(Type::Pointer(PointerType {
                    pointee: resolved_pointee,
                    span: p.span.clone(),
                }))
            }

            // Primitives - already resolved, just clone
            Type::Unit(_)
            | Type::Boolean(_)
            | Type::Byte(_)
            | Type::I32(_)
            | Type::I64(_)
            | Type::Decimal(_)
            | Type::String(_)
            | Type::Character(_) => Ok(ty.clone()),
        }
    }
}

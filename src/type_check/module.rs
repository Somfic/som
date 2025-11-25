use crate::{
    ast::{Declaration, FunctionType, PointerType, StructField, StructType, Type, Visibility},
    lexer::Path,
    Module, Result, TypeCheck, TypeCheckContext, TypeCheckError, Typed, Untyped,
};
use std::collections::HashMap;

#[derive(Default)]
pub struct Typer {
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

impl Typer {
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
        let mut ctx = TypeCheckContext::new(&self.registry);

        for (name, ty) in &scope.module_types {
            ctx.declare_type(name.clone(), ty.clone());
        }

        let mut resolved_types = Vec::new();

        for file in &module.files {
            for declaration in &file.declarations {
                match declaration {
                    Declaration::TypeDefinition(type_definition) => {
                        if matches!(type_definition.visibility, Visibility::Private) {
                            continue;
                        }

                        let resolved_type = self.resolve_type_for_definition(&type_definition.ty, &ctx, &type_definition.name.to_string())?;
                        let name = type_definition.name.to_string();

                        resolved_types.push((
                            name.clone(),
                            type_definition.visibility.clone(),
                            resolved_type.clone(),
                        ));

                        ctx.declare_type(name, resolved_type);
                    }
                    _ => { /* we only define types */ }
                }
            }
        }

        let scope = self
            .registry
            .get_mut(path)
            .expect("to be filled in by previous pass");

        for (name, visibility, resolved_type) in resolved_types {
            match visibility {
                Visibility::Private => unreachable!(),
                Visibility::Module => {
                    scope.module_types.insert(name, resolved_type);
                }
                Visibility::Public => {
                    scope
                        .public_types
                        .insert(name.clone(), resolved_type.clone());
                    scope.module_types.insert(name, resolved_type);
                }
            }
        }

        // Second pass: now that all types are in the registry, resolve any remaining Forwards
        let scope = self.registry.get(path).expect("to be filled in by first pass");
        let mut ctx = TypeCheckContext::new(&self.registry);

        for (name, ty) in &scope.module_types {
            ctx.declare_type(name.clone(), ty.clone());
        }

        let mut fully_resolved_types = Vec::new();
        let scope = self.registry.get(path).expect("registry");
        for (name, ty) in &scope.module_types {
            let fully_resolved = self.resolve_type(ty, &ctx)?;
            fully_resolved_types.push((name.clone(), fully_resolved));
        }

        let scope = self.registry.get_mut(path).expect("registry");
        for (name, fully_resolved) in fully_resolved_types {
            scope.module_types.insert(name.clone(), fully_resolved.clone());
            if scope.public_types.contains_key(&name) {
                scope.public_types.insert(name, fully_resolved);
            }
        }

        Ok(())
    }

    fn check_module(&mut self, module: Module<Untyped>) -> Result<Module<Typed>> {
        let scope = self
            .registry
            .get(&module.path)
            .expect("to be filled in by previous pass");

        let mut ctx = TypeCheckContext::new(&self.registry);

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
        self.resolve_type_with_depth(ty, ctx, 0, None)
    }

    fn resolve_type_for_definition(&self, ty: &Type, ctx: &TypeCheckContext, defining_type: &str) -> Result<Type> {
        self.resolve_type_with_depth(ty, ctx, 0, Some(defining_type))
    }

    fn resolve_type_with_depth(
        &self,
        ty: &Type,
        ctx: &TypeCheckContext,
        depth: usize,
        defining_type: Option<&str>,
    ) -> Result<Type> {
        const MAX_DEPTH: usize = 100;

        if depth > MAX_DEPTH {
            return Err(TypeCheckError::RecursiveType
                .to_diagnostic()
                .with_hint("Type definition contains infinite recursion"));
        }

        match ty {
            Type::Forward(forward) => {
                let actual_type = ctx.get_type_with_span(forward.name.to_string(), &forward.span)?;

                match &actual_type {
                    Type::Forward(_) => {
                        // Check if this is a self-reference (the type we're defining references itself)
                        if let Some(def_type) = defining_type {
                            if forward.name.to_string() == def_type {
                                // this is a directly recursive type like: type A = { field ~ A }
                                return TypeCheckError::RecursiveType
                                    .to_diagnostic()
                                    .with_label(forward.span.label("recursive type reference"))
                                    .with_hint(format!("Type '{}' contains itself directly, which creates infinite recursion. Use a pointer (*{}) to break the cycle.", forward.name, forward.name))
                                    .to_err();
                            }
                        }

                        // Not self-referential, just not resolved yet - return the Forward as-is
                        Ok(actual_type.clone())
                    }
                    _ => self.resolve_type_with_depth(&actual_type, ctx, depth + 1, defining_type),
                }
            }

            Type::Struct(s) => {
                let mut resolved_fields = Vec::new();
                for field in &s.fields {
                    let resolved_field_type =
                        self.resolve_type_with_depth(&field.ty, ctx, depth + 1, defining_type)?;
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

            Type::Function(f) => {
                let resolved_params = f
                    .parameters
                    .iter()
                    .map(|param| self.resolve_type_with_depth(param, ctx, depth + 1, defining_type))
                    .collect::<Result<Vec<_>>>()?;

                let resolved_return =
                    Box::new(self.resolve_type_with_depth(&f.returns, ctx, depth + 1, defining_type)?);

                Ok(Type::Function(FunctionType {
                    parameters: resolved_params,
                    returns: resolved_return,
                    span: f.span.clone(),
                }))
            }

            Type::Pointer(p) => {
                let resolved_pointee =
                    Box::new(self.resolve_type_with_depth(&p.pointee, ctx, depth + 1, defining_type)?);
                Ok(Type::Pointer(PointerType {
                    pointee: resolved_pointee,
                    span: p.span.clone(),
                }))
            }

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

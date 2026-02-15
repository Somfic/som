use std::collections::HashMap;
use std::fmt::Display;
use std::path::PathBuf;

mod expr;
pub use expr::*;
mod stmt;
pub use stmt::*;
mod decl;
pub use decl::*;
mod ty;
pub use ty::*;

use crate::arena::{Arena, Id};
use crate::span::Span;
use crate::type_check::{Constraint, TypeError};

#[derive(Clone, Debug)]
pub struct FuncSignature {
    pub params: Vec<Type>,
    pub return_type: Type,
}

#[derive(Clone, Debug)]
pub enum FuncKind {
    Regular(Id<Func>),
    Extern(Id<ExternFunc>),
}

#[derive(Clone, Debug)]
pub struct FuncEntry {
    pub signature: FuncSignature,
    pub kind: FuncKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    pub id: u32,
    pub value: Box<str>,
}

impl Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Path(pub Vec<Ident>);

impl Path {
    pub fn name(&self) -> &Ident {
        self.0.last().unwrap()
    }
    pub fn qualifier(&self) -> &[Ident] {
        &self.0[..self.0.len() - 1]
    }
    pub fn is_qualified(&self) -> bool {
        self.0.len() > 1
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let path_str = self
            .0
            .iter()
            .map(|ident| ident.value.as_ref())
            .collect::<Vec<_>>()
            .join("::");
        write!(f, "{}", path_str)
    }
}

#[derive(Default)]
pub struct TypedAst {
    pub ast: Ast,
    pub types: HashMap<Id<Expr>, Type>,
    pub errors: Vec<TypeError>,
    pub constraints: Vec<Constraint>,
}

impl TypedAst {
    pub fn get_expr_ty(&self, expr: &Id<Expr>) -> &Type {
        self.types.get(expr).unwrap()
    }
}

#[derive(Default)]
pub struct Ast {
    pub mods: Vec<Module>,

    pub exprs: Arena<Expr>,
    expr_spans: HashMap<Id<Expr>, Id<Span>>,

    pub stmts: Arena<Stmt>,
    stmt_spans: HashMap<Id<Stmt>, Id<Span>>,

    pub structs: Arena<Struct>,
    struct_spans: HashMap<Id<Struct>, Id<Span>>,

    pub uses: Arena<Use>,
    uses_spans: HashMap<Id<Use>, Id<Span>>,

    pub funcs: Arena<Func>,
    func_spans: HashMap<Id<Func>, Id<Span>>,

    pub extern_funcs: Arena<ExternFunc>,
    extern_func_spans: HashMap<Id<ExternFunc>, Id<Span>>,

    pub traits: Arena<Trait>,
    pub impls: Arena<Impl>,

    type_spans: HashMap<Id<Type>, Id<Span>>,
    pub spans: Arena<Span>,

    next_type_id: usize,

    pub func_registry: HashMap<String, FuncEntry>,
}

impl Ast {
    pub fn new() -> Self {
        let mut ast = Self::default();

        ast.impls.alloc(Impl {
            trait_id: TRAIT_ADD,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_SUB,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_MUL,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_DIV,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32, // TODO: f32?
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_LT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_GT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_LT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_GT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.impls.alloc(Impl {
            trait_id: TRAIT_NEQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        // pointer arthimetic (for raw pointers only, not references)
        ast.impls.alloc(Impl {
            trait_id: TRAIT_ADD,
            self_type: Type::Pointer,
            arg_types: vec![Type::I32],
            output_type: Type::Pointer,
        });

        ast
    }

    pub fn alloc_expr_with_span(&mut self, expr: Expr, span: Span) -> Id<Expr> {
        let id = self.exprs.alloc(expr);
        let span_id = self.spans.alloc(span);
        self.expr_spans.insert(id, span_id);
        id
    }

    pub fn alloc_stmt_with_span(&mut self, stmt: Stmt, span: Span) -> Id<Stmt> {
        let id = self.stmts.alloc(stmt);
        let span_id = self.spans.alloc(span);
        self.stmt_spans.insert(id, span_id);
        id
    }

    pub fn alloc_func_with_span(
        &mut self,
        func: Func,
        span: Span,
        module_name: Option<&str>,
    ) -> Id<Func> {
        let func_name = func.name.value.to_string();
        let signature = FuncSignature {
            params: func
                .parameters
                .iter()
                .map(|p| p.ty.clone().unwrap_or(Type::Unit))
                .collect(),
            return_type: func.return_type.clone().unwrap_or(Type::Unit),
        };

        let id = self.funcs.alloc(func);
        let span_id = self.spans.alloc(span);
        self.func_spans.insert(id, span_id);

        let registry_key = match module_name {
            Some(module) => format!("{}::{}", module, func_name),
            None => func_name,
        };

        self.func_registry.insert(
            registry_key,
            FuncEntry {
                signature,
                kind: FuncKind::Regular(id),
            },
        );

        id
    }

    pub fn alloc_extern_func_with_span(
        &mut self,
        func: ExternFunc,
        span: Span,
        module_name: Option<&str>,
    ) -> Id<ExternFunc> {
        let func_name = func.name.value.to_string();
        let signature = FuncSignature {
            params: func
                .parameters
                .iter()
                .map(|p| p.ty.clone().unwrap_or(Type::Unit))
                .collect(),
            return_type: func.return_type.clone().unwrap_or(Type::Unit),
        };

        let id = self.extern_funcs.alloc(func);
        let span_id = self.spans.alloc(span);
        self.extern_func_spans.insert(id, span_id);

        let registry_key = match module_name {
            Some(module) => format!("{}::{}", module, func_name),
            None => func_name,
        };

        self.func_registry.insert(
            registry_key,
            FuncEntry {
                signature,
                kind: FuncKind::Extern(id),
            },
        );

        id
    }

    pub fn alloc_struct_with_span(&mut self, struct_decl: Struct, span: Span) -> Id<Struct> {
        let id = self.structs.alloc(struct_decl);
        let span_id = self.spans.alloc(span);
        self.struct_spans.insert(id, span_id);
        id
    }

    pub fn alloc_use_with_span(&mut self, use_decl: Use, span: Span) -> Id<Use> {
        let id = self.uses.alloc(use_decl);
        let span_id = self.spans.alloc(span);
        self.uses_spans.insert(id, span_id);
        id
    }

    pub fn alloc_type_with_span(&mut self, span: Span) -> Id<Type> {
        let id = Id::<Type>::new(self.next_type_id);
        self.next_type_id += 1;
        let span_id = self.spans.alloc(span);
        self.type_spans.insert(id, span_id);
        id
    }

    pub fn find_func_by_path(&self, path: &Path) -> Option<Id<Func>> {
        if path.is_qualified() {
            // get module name (e.g., "std" from "std::print")
            let module_name = path.qualifier()[0].value.as_ref();
            let func_name = path.name().value.as_ref();

            // find the module
            let module = self.mods.iter().find(|m| &*m.name == module_name)?;

            // search only in that module's declarations
            for decl in &module.decs {
                if let Decl::Func(func_id) = decl {
                    let func = self.funcs.get(func_id);
                    if &*func.name.value == func_name {
                        return Some(*func_id);
                    }
                }
            }
            None
        } else {
            // unqualified: search globally (existing behavior)
            self.find_func_by_name(path.name().value.as_ref())
        }
    }

    pub fn find_func_by_name(&self, name: &str) -> Option<Id<Func>> {
        self.funcs
            .iter()
            .enumerate()
            .find(|(_, f)| &*f.name.value == name)
            .map(|(idx, _)| Id::new(idx))
    }

    pub fn find_extern_func_by_name(&self, name: &str) -> Option<Id<ExternFunc>> {
        self.extern_funcs
            .iter()
            .enumerate()
            .find(|(_, f)| &*f.name.value == name)
            .map(|(idx, _)| Id::new(idx))
    }

    pub fn find_struct_by_name(&self, name: &str) -> Option<Id<Struct>> {
        self.structs
            .iter()
            .enumerate()
            .find(|(_, s)| &*s.name.value == name)
            .map(|(idx, _)| Id::new(idx))
    }

    pub fn get_expr_span(&self, id: &Id<Expr>) -> &Span {
        let span_id = self.expr_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_stmt_span(&self, id: &Id<Stmt>) -> &Span {
        let span_id = self.stmt_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_func_span(&self, id: &Id<Func>) -> &Span {
        let span_id = self.func_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_type_span(&self, id: &Id<Type>) -> &Span {
        let span_id = self.type_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_use_span(&self, id: &Id<Use>) -> &Span {
        let span_id = self.uses_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_struct_span(&self, id: &Id<Struct>) -> &Span {
        let span_id = self.struct_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn get_extern_func_span(&self, id: &Id<ExternFunc>) -> &Span {
        let span_id = self.extern_func_spans.get(id).unwrap();
        self.spans.get(span_id)
    }

    pub fn try_get_type_span(&self, id: &Id<Type>) -> Option<&Span> {
        self.type_spans
            .get(id)
            .map(|span_id| self.spans.get(span_id))
    }

    pub fn find_impl(
        &self,
        trait_id: Id<Trait>,
        self_type: &Type,
        arg_types: &[Type],
    ) -> Option<&Impl> {
        self.impls.iter().find(|impl_def| {
            impl_def.trait_id == trait_id
                && impl_def.self_type == *self_type
                && impl_def.arg_types == arg_types
        })
    }
}

pub struct Module {
    pub name: Box<str>,
    pub path: PathBuf,
    pub decs: Vec<Decl>,
}

impl Module {
    pub fn uses<'ast>(&self, ast: &'ast Ast) -> impl Iterator<Item = &'ast Use> {
        self.decs.iter().filter_map(|d| match d {
            Decl::Use(id) => Some(ast.uses.get(id)),
            _ => None,
        })
    }
}

impl Ast {
    pub fn get_extern_libraries(&self) -> (Vec<String>, bool) {
        let mut libraries = Vec::new();
        let mut needs_libc = false;

        for module in &self.mods {
            for decl in &module.decs {
                if let Decl::ExternBlock(block) = decl {
                    match &block.library {
                        Some(lib) => libraries.push(lib.clone()),
                        None => needs_libc = true,
                    }
                }
            }
        }

        (libraries, needs_libc)
    }
}

const BUILTIN_TRAIT_COUNT: u32 = 100;

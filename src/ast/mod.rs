use std::collections::HashMap;

mod expr;
pub use expr::*;
mod stmt;
pub use stmt::*;
mod decl;
pub use decl::*;
mod ty;
pub use ty::*;

use crate::span::Span;
use crate::type_check::{Constraint, TypeError};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpanId(u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    pub id: u32,
    pub value: Box<str>,
}

#[derive(Default)]
pub struct TypedAst {
    pub ast: Ast,
    pub types: HashMap<ExprId, Type>,
    pub errors: HashMap<ExprId, TypeError>,
    pub constraints: Vec<Constraint>,
}

#[derive(Default)]
pub struct Ast {
    pub mods: Vec<Module>,

    pub exprs: Vec<Expr>,
    expr_spans: HashMap<ExprId, SpanId>,

    pub stmts: Vec<Stmt>,
    stmt_spans: HashMap<StmtId, SpanId>,

    pub funcs: Vec<Func>,
    func_spans: HashMap<FuncId, SpanId>,

    pub traits: Vec<Trait>,
    pub impls: Vec<Impl>,

    type_spans: HashMap<TypeId, SpanId>,
    pub spans: Vec<Span>,

    next_type_id: u32,
}

impl Ast {
    pub fn new() -> Self {
        let mut ast = Self::default();

        ast.alloc_impl(Impl {
            trait_id: TRAIT_ADD,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_SUB,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_MUL,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_DIV,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32, // TODO: f32?
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_LT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_GT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_LT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_GT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(Impl {
            trait_id: TRAIT_NEQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast
    }

    pub fn alloc_expr(&mut self, expr: Expr) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(expr);
        id
    }

    pub fn alloc_expr_with_span(&mut self, expr: Expr, span: Span) -> ExprId {
        let id = self.alloc_expr(expr);
        let span_id = self.alloc_span(span);
        self.expr_spans.insert(id, span_id);
        id
    }

    pub fn alloc_stmt(&mut self, stmt: Stmt) -> StmtId {
        let id = StmtId(self.stmts.len() as u32);
        self.stmts.push(stmt);
        id
    }

    pub fn alloc_stmt_with_span(&mut self, stmt: Stmt, span: Span) -> StmtId {
        let id = self.alloc_stmt(stmt);
        let span_id = self.alloc_span(span);
        self.stmt_spans.insert(id, span_id);
        id
    }

    pub fn alloc_func(&mut self, func: Func) -> FuncId {
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
        id
    }

    pub fn alloc_func_with_span(&mut self, func: Func, span: Span) -> FuncId {
        let id = self.alloc_func(func);
        let span_id = self.alloc_span(span);
        self.func_spans.insert(id, span_id);
        id
    }

    pub fn alloc_trait(&mut self, r#trait: Trait) -> TraitId {
        let id = TraitId(BUILTIN_TRAIT_COUNT + self.traits.len() as u32);
        self.traits.push(r#trait);
        id
    }

    pub fn alloc_impl(&mut self, r#impl: Impl) -> ImplId {
        let id = ImplId(self.impls.len() as u32);
        self.impls.push(r#impl);
        id
    }

    pub fn alloc_type_with_span(&mut self, span: Span) -> TypeId {
        let id = TypeId(self.next_type_id);
        self.next_type_id += 1;
        let span_id = self.alloc_span(span);
        self.type_spans.insert(id, span_id);
        id
    }

    pub fn alloc_span(&mut self, span: Span) -> SpanId {
        let id = SpanId(self.spans.len() as u32);
        self.spans.push(span);
        id
    }

    pub fn get_expr(&self, id: &ExprId) -> &Expr {
        self.exprs.get(id.0 as usize).unwrap()
    }

    pub fn get_stmt(&self, id: &StmtId) -> &Stmt {
        self.stmts.get(id.0 as usize).unwrap()
    }

    pub fn get_func(&self, id: &FuncId) -> &Func {
        self.funcs.get(id.0 as usize).unwrap()
    }

    pub fn get_trait(&self, id: &TraitId) -> &Trait {
        self.traits.get(id.0 as usize).unwrap()
    }

    pub fn get_impl(&self, id: &ImplId) -> &Impl {
        self.impls.get(id.0 as usize).unwrap()
    }

    pub fn get_span(&self, id: &SpanId) -> &Span {
        self.spans.get(id.0 as usize).unwrap()
    }

    pub fn get_expr_span(&self, id: &ExprId) -> &Span {
        let span_id = self.expr_spans.get(id).unwrap();
        self.get_span(span_id)
    }

    pub fn get_stmt_span(&self, id: &StmtId) -> &Span {
        let span_id = self.stmt_spans.get(id).unwrap();
        self.get_span(span_id)
    }

    pub fn get_func_span(&self, id: &FuncId) -> &Span {
        let span_id = self.func_spans.get(id).unwrap();
        self.get_span(span_id)
    }

    pub fn get_type_span(&self, id: &TypeId) -> &Span {
        let span_id = self.type_spans.get(id).unwrap();
        self.get_span(span_id)
    }

    pub fn try_get_type_span(&self, id: &TypeId) -> Option<&Span> {
        self.type_spans.get(id).map(|span_id| self.get_span(span_id))
    }

    pub fn find_impl(
        &self,
        trait_id: TraitId,
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
    pub decs: Vec<Decl>,
}

const BUILTIN_TRAIT_COUNT: u32 = 100;

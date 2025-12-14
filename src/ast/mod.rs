use std::collections::HashMap;
use std::fmt::Display;

use ena::unify::{UnifyKey, UnifyValue};

mod expr;
pub use expr::*;
mod stmt;
pub use stmt::*;

use crate::span::Span;
use crate::type_check::{Constraint, TypeError};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImplId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

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

    // expressions and statements
    exprs: Vec<Expr>,
    stmts: Vec<Stmt>,

    // declarations
    pub funcs: Vec<FuncDec>,
    pub traits: Vec<TraitDec>,
    // structs: Vec<StructDec>,
    pub impls: Vec<ImplDec>,

    // span tracking
    pub expr_spans: HashMap<ExprId, Span>,
    pub stmt_spans: HashMap<StmtId, Span>,
    pub func_spans: HashMap<FuncId, Span>,
    pub type_spans: HashMap<TypeId, Span>,

    // type allocation counter
    next_type_id: u32,
}

impl Ast {
    pub fn new() -> Self {
        let mut ast = Self::default();

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_ADD,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_SUB,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_MUL,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_DIV,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::I32, // TODO: f32?
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_LT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_GT,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_LT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_GT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(ImplDec {
            trait_id: TRAIT_EQ,
            self_type: Type::I32,
            arg_types: vec![Type::I32],
            output_type: Type::Bool,
        });

        ast.alloc_impl(ImplDec {
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
        self.expr_spans.insert(id, span);
        id
    }

    pub fn alloc_stmt(&mut self, stmt: Stmt) -> StmtId {
        let id = StmtId(self.stmts.len() as u32);
        self.stmts.push(stmt);
        id
    }

    pub fn alloc_stmt_with_span(&mut self, stmt: Stmt, span: Span) -> StmtId {
        let id = self.alloc_stmt(stmt);
        self.stmt_spans.insert(id, span);
        id
    }

    pub fn alloc_func(&mut self, func: FuncDec) -> FuncId {
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
        id
    }

    pub fn alloc_func_with_span(&mut self, func: FuncDec, span: Span) -> FuncId {
        let id = self.alloc_func(func);
        self.func_spans.insert(id, span);
        id
    }

    pub fn alloc_trait(&mut self, trait_dec: TraitDec) -> TraitId {
        let id = TraitId(BUILTIN_TRAIT_COUNT + self.traits.len() as u32);
        self.traits.push(trait_dec);
        id
    }

    pub fn alloc_impl(&mut self, impl_dec: ImplDec) -> ImplId {
        let id = ImplId(self.impls.len() as u32);
        self.impls.push(impl_dec);
        id
    }

    pub fn alloc_type_with_span(&mut self, span: Span) -> TypeId {
        let id = TypeId(self.next_type_id);
        self.next_type_id += 1;
        self.type_spans.insert(id, span);
        id
    }

    pub fn get_expr(&self, id: &ExprId) -> &Expr {
        self.exprs.get(id.0 as usize).unwrap()
    }

    pub fn get_stmt(&self, id: &StmtId) -> &Stmt {
        self.stmts.get(id.0 as usize).unwrap()
    }

    pub fn get_func(&self, id: &FuncId) -> &FuncDec {
        self.funcs.get(id.0 as usize).unwrap()
    }

    pub fn get_trait(&self, id: &TraitId) -> &TraitDec {
        self.traits.get(id.0 as usize).unwrap()
    }

    pub fn get_impl(&self, id: &ImplId) -> &ImplDec {
        self.impls.get(id.0 as usize).unwrap()
    }

    pub fn find_impl(
        &self,
        trait_id: TraitId,
        self_type: &Type,
        arg_types: &[Type],
    ) -> Option<&ImplDec> {
        self.impls.iter().find(|impl_def| {
            impl_def.trait_id == trait_id
                && impl_def.self_type == *self_type
                && impl_def.arg_types == arg_types
        })
    }
}

pub struct Module {
    pub name: Box<str>,
    pub decs: Vec<DecId>,
}

pub enum DecId {
    Func(FuncId),
    Trait(TraitId),
    Struct(StructId),
    Impl(ImplId),
}

pub struct FuncDec {
    pub name: Ident,
    pub parameters: Vec<FuncParam>,
    pub return_type: Option<Type>,
    pub return_type_id: Option<TypeId>, // TypeId for the return type annotation (for span tracking)
    pub body: ExprId,
}

pub struct TraitDec {
    pub name: Ident,
    pub parameters: Vec<FuncParam>,
    pub returns: Type,
}

pub struct ImplDec {
    pub trait_id: TraitId,
    pub self_type: Type,
    pub arg_types: Vec<Type>,
    pub output_type: Type,
}

pub struct FuncParam {
    pub name: Ident,
    pub ty: Option<Type>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypeVar(pub u32);

impl UnifyKey for TypeVar {
    type Value = TypeValue;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        TypeVar(u)
    }

    fn tag() -> &'static str {
        "TypeVar"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeValue {
    Bound(Type),
    Unbound,
}

impl UnifyValue for TypeValue {
    type Error = ();

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        match (a, b) {
            // Both unbound - stay unbound
            (TypeValue::Unbound, TypeValue::Unbound) => Ok(TypeValue::Unbound),

            // One bound, one unbound - take the bound one
            (TypeValue::Bound(ty), TypeValue::Unbound)
            | (TypeValue::Unbound, TypeValue::Bound(ty)) => Ok(TypeValue::Bound(ty.clone())),

            // Both bound - must be identical
            (TypeValue::Bound(t1), TypeValue::Bound(t2)) => {
                if t1 == t2 {
                    Ok(TypeValue::Bound(t1.clone()))
                } else {
                    Err(()) // Type mismatch
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifetimeVar(pub u32);

impl UnifyKey for LifetimeVar {
    type Value = LifetimeValue;

    fn index(&self) -> u32 {
        self.0
    }

    fn from_index(u: u32) -> Self {
        LifetimeVar(u)
    }

    fn tag() -> &'static str {
        "LifetimeVar"
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifetimeValue {
    Bound(Lifetime),
    Unbound,
}

impl UnifyValue for LifetimeValue {
    type Error = ();

    fn unify_values(a: &Self, b: &Self) -> Result<Self, Self::Error> {
        match (a, b) {
            // Both unbound - stay unbound
            (LifetimeValue::Unbound, LifetimeValue::Unbound) => Ok(LifetimeValue::Unbound),

            // One bound, one unbound - take the bound one
            (LifetimeValue::Bound(lt), LifetimeValue::Unbound)
            | (LifetimeValue::Unbound, LifetimeValue::Bound(lt)) => {
                Ok(LifetimeValue::Bound(lt.clone()))
            }

            // Both bound - must be identical
            (LifetimeValue::Bound(l1), LifetimeValue::Bound(l2)) => {
                if l1 == l2 {
                    Ok(LifetimeValue::Bound(l1.clone()))
                } else {
                    Err(()) // Lifetime mismatch
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifetime {
    Unknown(LifetimeVar),
    Unspecified,
    Named(Box<str>),
    Static,
}

impl Display for Lifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lifetime::Unknown(var) => write!(f, "'{}", var.0),
            Lifetime::Unspecified => write!(f, "'_"),
            Lifetime::Named(name) => write!(f, "'{}", name),
            Lifetime::Static => write!(f, "'static"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Unknown(TypeVar),
    I32,
    Bool,
    Str,
    Reference {
        mutable: bool,
        lifetime: Lifetime,
        to: Box<Type>,
    },
    Fun {
        arguments: Vec<Type>,
        returns: Box<Type>,
    },
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Unit => write!(f, "()"),
            Type::Unknown(var) => write!(f, "T{}", var.0),
            Type::Bool => write!(f, "bool"),
            Type::I32 => write!(f, "i32"),
            Type::Str => write!(f, "str"),
            Type::Reference {
                mutable,
                lifetime,
                to,
            } => {
                let lifetime = if let Lifetime::Unspecified = lifetime {
                    ""
                } else {
                    &format!("{} ", lifetime)
                };

                if *mutable {
                    write!(f, "&mut{}{}", lifetime, to)
                } else {
                    write!(f, "&{}{}", lifetime, to)
                }
            }
            Type::Fun { arguments, returns } => {
                let args: Vec<String> = arguments.iter().map(|arg| format!("{}", arg)).collect();
                write!(f, "fn({}) -> {}", args.join(", "), returns)
            }
        }
    }
}

const BUILTIN_TRAIT_COUNT: u32 = 100;

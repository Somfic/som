use ena::unify::{UnifyKey, UnifyValue};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ExprId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StmtId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct TraitId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImplId(pub u32);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    pub id: u32,
    pub value: Box<str>,
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

        ast
    }

    pub fn alloc_expr(&mut self, expr: Expr) -> ExprId {
        let id = ExprId(self.exprs.len() as u32);
        self.exprs.push(expr);
        id
    }

    pub fn alloc_stmt(&mut self, stmt: Stmt) -> StmtId {
        let id = StmtId(self.stmts.len() as u32);
        self.stmts.push(stmt);
        id
    }

    pub fn alloc_func(&mut self, func: FuncDec) -> FuncId {
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(func);
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
    pub ty: Type,
}

pub enum Stmt {
    Let {
        name: Ident,
        ty: Option<Type>,
        value: ExprId,
    },
}

pub enum Expr {
    I32(i32),
    Var(Ident),
    Binary {
        op: BinOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Block {
        stmts: Vec<StmtId>,
        value: Option<ExprId>,
    },
}

pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    LessThan,
    GreaterThan,
    Equals,
    NotEquals,
    And,
    Or,
}

impl BinOp {
    pub fn trait_id(&self) -> TraitId {
        match self {
            BinOp::Add => TRAIT_ADD,
            BinOp::Subtract => TRAIT_SUB,
            BinOp::Multiply => TRAIT_MUL,
            BinOp::Divide => TRAIT_DIV,
            BinOp::LessThan => TRAIT_LT,
            BinOp::GreaterThan => TRAIT_GT,
            BinOp::Equals => TRAIT_EQ,
            BinOp::NotEquals => TRAIT_NEQ,
            BinOp::And => TRAIT_AND,
            BinOp::Or => TRAIT_OR,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Unit,
    Unknown(TypeVar),
    Bool,
    I32,
    Fun {
        arguments: Vec<Type>,
        returns: Box<Type>,
    },
}

pub const TRAIT_ADD: TraitId = TraitId(0);
pub const TRAIT_SUB: TraitId = TraitId(1);
pub const TRAIT_MUL: TraitId = TraitId(2);
pub const TRAIT_DIV: TraitId = TraitId(3);
pub const TRAIT_EQ: TraitId = TraitId(4);
pub const TRAIT_NEQ: TraitId = TraitId(5);
pub const TRAIT_LT: TraitId = TraitId(6);
pub const TRAIT_GT: TraitId = TraitId(7);
pub const TRAIT_AND: TraitId = TraitId(8);
pub const TRAIT_OR: TraitId = TraitId(9);
const BUILTIN_TRAIT_COUNT: u32 = 100;

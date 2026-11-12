use som_ast::{Ast, Expr, Func, Id, Span, Stmt, Struct, TypeExpr};
use std::collections::HashMap;

/// A definition: anything a name can refer to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DefId(pub u32);

#[derive(Clone, Debug)]
pub enum DefKind {
    /// A function, method or extern function.
    Func(Id<Func>),
    Struct(Id<Struct>),
    /// The `index`th parameter of `func`.
    Param {
        func: Id<Func>,
        index: usize,
    },
    /// A `let` binding.
    Local(Id<Stmt>),
    /// The `index`th type parameter of `func`, as in `fn id<T>(x: T) -> T`.
    TypeParam {
        func: Id<Func>,
        index: usize,
    },
}

#[derive(Clone, Debug)]
pub struct Def {
    pub name: Box<str>,
    pub kind: DefKind,
    pub span: Span,
    /// Index into `Ast::modules` of the module this is defined in.
    pub module: usize,
}

/// The result of name resolution: what every name in the program refers to.
///
/// Side tables are indexed by the AST's own ids, so the AST itself stays untouched.
pub struct Names {
    pub defs: Vec<Def>,
    /// `Expr::Var`, `Expr::Call` and `Expr::Construct` → what their path names.
    exprs: Vec<Option<DefId>>,
    /// `TypeExpr::Named` → the struct or type parameter it names.
    types: Vec<Option<DefId>>,
    /// `Stmt::Let` → the local it introduces.
    lets: Vec<Option<DefId>>,
    /// Each function's own definition and its parameters.
    funcs: Vec<Option<DefId>>,
    params: Vec<Vec<DefId>>,
    type_params: Vec<Vec<DefId>>,
    structs: Vec<Option<DefId>>,
    /// `(struct, method name)` → the method, for `impl` blocks.
    methods: HashMap<(DefId, Box<str>), DefId>,
}

impl Names {
    pub(crate) fn new(ast: &Ast) -> Names {
        Names {
            defs: Vec::new(),
            exprs: vec![None; ast.exprs.len()],
            types: vec![None; ast.types.len()],
            lets: vec![None; ast.stmts.len()],
            funcs: vec![None; ast.funcs.len()],
            params: vec![Vec::new(); ast.funcs.len()],
            type_params: vec![Vec::new(); ast.funcs.len()],
            structs: vec![None; ast.structs.len()],
            methods: HashMap::new(),
        }
    }

    pub(crate) fn define(&mut self, name: &str, kind: DefKind, span: Span, module: usize) -> DefId {
        let id = DefId(self.defs.len() as u32);
        self.defs.push(Def {
            name: name.into(),
            kind,
            span,
            module,
        });
        id
    }

    pub(crate) fn set_func(&mut self, func: Id<Func>, def: DefId) {
        self.funcs[func.id] = Some(def);
    }

    pub(crate) fn set_struct(&mut self, s: Id<Struct>, def: DefId) {
        self.structs[s.id] = Some(def);
    }

    pub(crate) fn set_expr(&mut self, expr: Id<Expr>, def: DefId) {
        self.exprs[expr.id] = Some(def);
    }

    pub(crate) fn set_type(&mut self, ty: Id<TypeExpr>, def: DefId) {
        self.types[ty.id] = Some(def);
    }

    pub(crate) fn set_let(&mut self, stmt: Id<Stmt>, def: DefId) {
        self.lets[stmt.id] = Some(def);
    }

    pub(crate) fn push_param(&mut self, func: Id<Func>, def: DefId) {
        self.params[func.id].push(def);
    }

    pub(crate) fn push_type_param(&mut self, func: Id<Func>, def: DefId) {
        self.type_params[func.id].push(def);
    }

    /// Returns the existing method if one with this name is already defined.
    pub(crate) fn add_method(&mut self, owner: DefId, name: &str, method: DefId) -> Option<DefId> {
        match self.methods.entry((owner, name.into())) {
            std::collections::hash_map::Entry::Occupied(e) => Some(*e.get()),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(method);
                None
            }
        }
    }

    // --- Queries ---

    pub fn def(&self, id: DefId) -> &Def {
        &self.defs[id.0 as usize]
    }

    /// What a `Var`, `Call` or `Construct` expression refers to. `None` if it failed to resolve.
    pub fn expr(&self, expr: Id<Expr>) -> Option<DefId> {
        self.exprs[expr.id]
    }

    /// What a `TypeExpr::Named` refers to. `None` if it failed to resolve.
    pub fn ty(&self, ty: Id<TypeExpr>) -> Option<DefId> {
        self.types[ty.id]
    }

    /// The local introduced by a `let` statement.
    pub fn local(&self, stmt: Id<Stmt>) -> Option<DefId> {
        self.lets[stmt.id]
    }

    /// The definition of a function. Every function in the AST has one.
    pub fn func(&self, func: Id<Func>) -> DefId {
        self.funcs[func.id].expect("every function is defined")
    }

    pub fn params(&self, func: Id<Func>) -> &[DefId] {
        &self.params[func.id]
    }

    /// The type parameters of a generic function, in declaration order.
    pub fn type_params(&self, func: Id<Func>) -> &[DefId] {
        &self.type_params[func.id]
    }

    /// The definition of a struct. Every struct in the AST has one.
    pub fn strukt(&self, s: Id<Struct>) -> DefId {
        self.structs[s.id].expect("every struct is defined")
    }

    /// Every method defined on `owner`, by name.
    pub fn methods_of(&self, owner: DefId) -> Vec<(&str, DefId)> {
        let mut methods: Vec<(&str, DefId)> = self
            .methods
            .iter()
            .filter(|((o, _), _)| *o == owner)
            .map(|((_, name), &def)| (&**name, def))
            .collect();
        methods.sort();
        methods
    }

    /// The method `name` defined on `owner` by an `impl` block.
    pub fn method(&self, owner: DefId, name: &str) -> Option<DefId> {
        self.methods.get(&(owner, name.into())).copied()
    }
}

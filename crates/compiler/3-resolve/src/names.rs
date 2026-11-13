use som_ast::{Ast, Expr, Func, Id, Pattern, Span, Stmt, Struct, TypeExpr};
use som_base::Arena;
use std::collections::HashMap;

/// A definition: anything a name can refer to.
pub type DefId = Id<Def>;

#[derive(Clone, Debug)]
pub enum DefKind {
    /// A function, method or extern function.
    Func(Id<Func>),
    /// A struct, or an enum.
    Struct(Id<Struct>),
    /// The `index`th variant of enum `strukt`: `Shape::Circle`.
    Variant { strukt: Id<Struct>, index: usize },
    /// A name a `match` arm's pattern binds: `r` in `Circle(r) => …`.
    Binding(Id<Pattern>),
    /// The `index`th parameter of `func`.
    Param { func: Id<Func>, index: usize },
    /// A `let` binding.
    Local(Id<Stmt>),
    /// A variable declared by an `out name` argument.
    Out(Id<Expr>),
    /// The `index`th generic parameter of `func`, as in `fn id<T>(x: T) -> T`. A `value`
    /// parameter, `N` in `fn sum<N: usize>(xs: [i32; N])`, stands for a value, not a type.
    TypeParam {
        func: Id<Func>,
        index: usize,
        value: bool,
    },
    /// The `index`th generic parameter of a struct, as in `struct Pair<T>`, or a value
    /// parameter, as in `struct Buffer<N: usize>`.
    StructParam {
        strukt: Id<Struct>,
        index: usize,
        value: bool,
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
    pub defs: Arena<Def>,
    /// `Expr::Var`, `Expr::Call` and `Expr::Construct` → what their path names.
    exprs: Vec<Option<DefId>>,
    /// `TypeExpr::Named` → the struct or type parameter it names.
    types: Vec<Option<DefId>>,
    /// `Stmt::Let` → the local it introduces.
    lets: Vec<Option<DefId>>,
    /// `break x;` and `continue x;` → the variable of the `for` loop they name.
    loop_targets: Vec<Option<DefId>>,
    /// Each function's own definition and its parameters.
    funcs: Vec<Option<DefId>>,
    params: Vec<Vec<DefId>>,
    type_params: Vec<Vec<DefId>>,
    /// Each struct's type parameters.
    struct_params: Vec<Vec<DefId>>,
    structs: Vec<Option<DefId>>,
    /// `(struct, method name)` → the method, for `impl` blocks.
    methods: HashMap<(DefId, Box<str>), DefId>,
    /// Each enum's variants, in order.
    variants: Vec<Vec<DefId>>,
    /// Patterns → the name they bind, or the variant a qualified one names. A bare variant
    /// name (`Circle(r)`, `Empty`) has none: the type matched decides which enum's it is.
    patterns: Vec<Option<DefId>>,
}

impl Names {
    pub(crate) fn new(ast: &Ast) -> Names {
        Names {
            defs: Arena::new(),
            exprs: vec![None; ast.exprs.len()],
            types: vec![None; ast.types.len()],
            lets: vec![None; ast.stmts.len()],
            loop_targets: vec![None; ast.stmts.len()],
            funcs: vec![None; ast.funcs.len()],
            params: vec![Vec::new(); ast.funcs.len()],
            type_params: vec![Vec::new(); ast.funcs.len()],
            struct_params: vec![Vec::new(); ast.structs.len()],
            structs: vec![None; ast.structs.len()],
            methods: HashMap::new(),
            variants: vec![Vec::new(); ast.structs.len()],
            patterns: vec![None; ast.patterns.len()],
        }
    }

    pub(crate) fn push_variant(&mut self, s: Id<Struct>, def: DefId) {
        self.variants[s.id].push(def);
    }

    pub(crate) fn set_pattern(&mut self, pattern: Id<Pattern>, def: DefId) {
        self.patterns[pattern.id] = Some(def);
    }

    pub(crate) fn define(&mut self, name: &str, kind: DefKind, span: Span, module: usize) -> DefId {
        self.defs.alloc(Def {
            name: name.into(),
            kind,
            span,
            module,
        })
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

    pub(crate) fn push_struct_param(&mut self, s: Id<Struct>, def: DefId) {
        self.struct_params[s.id].push(def);
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
        &self.defs[id]
    }

    /// Whether `def` is a value parameter, like `N` in `struct Buffer<N: usize>`.
    pub fn is_value_param(&self, def: DefId) -> bool {
        matches!(
            self.defs[def].kind,
            DefKind::TypeParam { value: true, .. } | DefKind::StructParam { value: true, .. }
        )
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

    pub fn set_loop_target(&mut self, stmt: Id<Stmt>, def: DefId) {
        self.loop_targets[stmt.id] = Some(def);
    }

    /// The `for` loop variable `break x;` or `continue x;` names.
    pub fn loop_target(&self, stmt: Id<Stmt>) -> Option<DefId> {
        self.loop_targets[stmt.id]
    }

    /// The definition of a function. Every function in the AST has one.
    pub fn func(&self, func: Id<Func>) -> DefId {
        self.funcs[func.id].expect("every function is defined")
    }

    pub fn params(&self, func: Id<Func>) -> &[DefId] {
        &self.params[func.id]
    }

    /// The generic parameters of a function, types and values, in declaration order.
    pub fn type_params(&self, func: Id<Func>) -> &[DefId] {
        &self.type_params[func.id]
    }

    /// The generic parameters of a struct, types and values, in declaration order.
    pub fn struct_params(&self, s: Id<Struct>) -> &[DefId] {
        &self.struct_params[s.id]
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

    /// An enum's variants, in order. Empty for a struct.
    pub fn variants(&self, s: Id<Struct>) -> &[DefId] {
        &self.variants[s.id]
    }

    /// Enum `owner`'s variant called `name`.
    pub fn variant(&self, owner: DefId, name: &str) -> Option<DefId> {
        let DefKind::Struct(s) = self.defs[owner].kind else {
            return None;
        };
        (self.variants[s.id].iter().copied()).find(|&v| &*self.defs[v].name == name)
    }

    /// What pattern `pattern` binds, or the variant it names if it's qualified: see `patterns`.
    pub fn pattern(&self, pattern: Id<Pattern>) -> Option<DefId> {
        self.patterns[pattern.id]
    }

    /// The method `name` defined on `owner` by an `impl` block.
    pub fn method(&self, owner: DefId, name: &str) -> Option<DefId> {
        self.methods.get(&(owner, name.into())).copied()
    }
}

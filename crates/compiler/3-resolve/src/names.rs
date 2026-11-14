use som_ast::{Ast, Expr, Func, Id, Pattern, Span, Stmt, Struct, Trait, TypeExpr};
use som_base::Arena;
use std::collections::HashMap;

/// A definition: anything a name can refer to.
pub type DefId = Id<Def>;

#[derive(Clone, Debug)]
pub enum DefKind {
    /// A function, method or extern function.
    Func(Id<Func>),
    /// A built-in type with methods of its own: `i32` in `impl i32 { … }`.
    Primitive(som_ast::Primitive),
    /// A struct, or an enum.
    Struct(Id<Struct>),
    /// The `index`th variant of enum `strukt`: `Shape::Circle`.
    Variant { strukt: Id<Struct>, index: usize },
    /// A name a `match` arm's pattern binds: `r` in `Circle(r) => …`.
    Binding(Id<Pattern>),
    /// `trait Iterator { … }`
    Trait(Id<Trait>),
    /// The `index`th associated type of a trait: `Item` in `trait Iterator { type Item; }`.
    AssocType { trait_: Id<Trait>, index: usize },
    /// The `index`th generic parameter of impl block `impl_` (see `Names::impls`), as its
    /// target and associated types are written: `T` in `impl<T> Iterator for ListIter<T>`.
    ImplParam {
        impl_: usize,
        index: usize,
        value: bool,
    },
    /// `Self` inside impl block `impl_`: its target type.
    SelfType { impl_: usize },
    /// The `index`th associated type impl block `impl_` says: `Item` in `type Item = &T;`.
    ImplType { impl_: usize, index: usize },
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

/// A trait the language itself uses, found by its name in the standard library: `for` loops
/// go through `Iterable` and `Iterator`, a value becomes another type through `From`, `==` is
/// `Eq`'s, `+` is `Add`'s, `<` is `Compare`'s. Adding one is adding a case here, and the hook where the language uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LangTrait {
    Iterator,
    Iterable,
    From,
    Eq,
    Add,
    Subtract,
    Multiply,
    Divide,
    Compare,
    Drop,
}

impl LangTrait {
    /// The trait operator `op` goes through, for values of a type that implements it.
    pub fn of_operator(op: som_ast::BinOp) -> Option<LangTrait> {
        use som_ast::BinOp;
        Some(match op {
            BinOp::Equals | BinOp::NotEquals => LangTrait::Eq,
            BinOp::Add => LangTrait::Add,
            BinOp::Subtract => LangTrait::Subtract,
            BinOp::Multiply => LangTrait::Multiply,
            BinOp::Divide => LangTrait::Divide,
            BinOp::LessThan
            | BinOp::GreaterThan
            | BinOp::LessThanOrEqual
            | BinOp::GreaterThanOrEqual => LangTrait::Compare,
            BinOp::Modulo | BinOp::And | BinOp::Or => return None,
        })
    }

    /// The method of it an operator calls.
    pub fn operator_method(self) -> Option<&'static str> {
        Some(match self {
            LangTrait::Eq => "eq",
            LangTrait::Add => "add",
            LangTrait::Subtract => "subtract",
            LangTrait::Multiply => "multiply",
            LangTrait::Divide => "divide",
            LangTrait::Compare => "compare",
            LangTrait::Iterator | LangTrait::Iterable | LangTrait::From | LangTrait::Drop => {
                return None;
            }
        })
    }

    pub const ALL: [LangTrait; 10] = [
        LangTrait::Iterator,
        LangTrait::Iterable,
        LangTrait::From,
        LangTrait::Eq,
        LangTrait::Add,
        LangTrait::Subtract,
        LangTrait::Multiply,
        LangTrait::Divide,
        LangTrait::Compare,
        LangTrait::Drop,
    ];

    /// Its name in the standard library.
    pub fn name(self) -> &'static str {
        match self {
            LangTrait::Iterator => "Iterator",
            LangTrait::Iterable => "Iterable",
            LangTrait::From => "From",
            LangTrait::Eq => "Eq",
            LangTrait::Add => "Add",
            LangTrait::Subtract => "Subtract",
            LangTrait::Multiply => "Multiply",
            LangTrait::Divide => "Divide",
            LangTrait::Compare => "Compare",
            LangTrait::Drop => "Drop",
        }
    }
}

/// An `impl` block, by its index in `Names::impls`.
pub struct ImplDef {
    pub module: usize,
    /// The trait it implements, if it does.
    pub trait_: Option<DefId>,
    /// The struct it's for.
    pub owner: Option<DefId>,
    /// Its generic parameters, as its target and associated types use them: `ImplParam`s.
    /// Each method has its own copies, first among its generics.
    pub params: Vec<DefId>,
    pub methods: Vec<Id<Func>>,
    /// The associated types it says, `ImplType`s, in the order written.
    pub types: Vec<DefId>,
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
    /// Functions sharing a name in a module: the first one's definition → all of them, in
    /// order. A call names the first; the type checker picks the one that fits.
    overloads: HashMap<DefId, Vec<DefId>>,
    /// Each trait's definition.
    traits: Vec<Option<DefId>>,
    /// Each trait's associated types, in order.
    trait_types: Vec<Vec<DefId>>,
    /// Every `impl` block, in the order they're written, module by module.
    pub impls: Vec<ImplDef>,
    /// A method → the impl block it's in.
    impl_of: HashMap<Id<Func>, usize>,
    /// A type parameter → the traits it has to implement.
    bounds: HashMap<DefId, Vec<DefId>>,
    /// `T::Item`, `Self::Item` → the type parameter or `Self` it's of.
    projection_bases: HashMap<Id<TypeExpr>, DefId>,
    /// The traits the language uses, where the standard library defines them.
    lang: HashMap<LangTrait, DefId>,
    /// Patterns → the name they bind, or the variant a qualified one names. A bare variant
    /// name (`Circle(r)`, `Empty`) has none: the type matched decides which enum's it is.
    patterns: Vec<Option<DefId>>,
    /// Built-in types with methods of their own, from `impl i32 { … }`.
    primitives: HashMap<som_ast::Primitive, DefId>,
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
            overloads: HashMap::new(),
            traits: vec![None; ast.traits.len()],
            trait_types: vec![Vec::new(); ast.traits.len()],
            impls: Vec::new(),
            impl_of: HashMap::new(),
            bounds: HashMap::new(),
            projection_bases: HashMap::new(),
            lang: HashMap::new(),
            patterns: vec![None; ast.patterns.len()],
            primitives: HashMap::new(),
        }
    }

    pub(crate) fn set_trait(&mut self, t: Id<Trait>, def: DefId, types: Vec<DefId>) {
        self.traits[t.id] = Some(def);
        self.trait_types[t.id] = types;
    }

    pub(crate) fn add_impl(&mut self, def: ImplDef) {
        for &method in &def.methods {
            self.impl_of.insert(method, self.impls.len());
        }
        self.impls.push(def);
    }

    pub(crate) fn set_bounds(&mut self, param: DefId, traits: Vec<DefId>) {
        self.bounds.insert(param, traits);
    }

    pub(crate) fn set_projection_base(&mut self, ty: Id<TypeExpr>, base: DefId) {
        self.projection_bases.insert(ty, base);
    }

    pub(crate) fn set_lang(&mut self, lang: LangTrait, def: DefId) {
        self.lang.insert(lang, def);
    }

    /// Adds `def` to the functions named like `first`.
    pub(crate) fn add_overload(&mut self, first: DefId, def: DefId) {
        self.overloads
            .entry(first)
            .or_insert_with(|| vec![first])
            .push(def);
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
            DefKind::TypeParam { value: true, .. }
                | DefKind::StructParam { value: true, .. }
                | DefKind::ImplParam { value: true, .. }
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

    /// The definition of a function, if it got one: one the parser gave up on part way, like
    /// `fn n` while it's typed, didn't.
    pub fn try_func(&self, func: Id<Func>) -> Option<DefId> {
        self.funcs[func.id]
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

    /// Every function named like function `def` in its module, `def` first; just `def` if it's
    /// the only one.
    pub fn overloads(&self, def: DefId) -> Vec<DefId> {
        match self.overloads.get(&def) {
            Some(all) => all.clone(),
            None => vec![def],
        }
    }

    /// The definition of a trait.
    pub fn trait_def(&self, t: Id<Trait>) -> DefId {
        self.traits[t.id].expect("every trait is defined")
    }

    /// A trait's associated types, in order.
    pub fn trait_types(&self, t: Id<Trait>) -> &[DefId] {
        &self.trait_types[t.id]
    }

    /// The impl block method `func` is in, by its index in `impls`.
    pub fn impl_of(&self, func: Id<Func>) -> Option<usize> {
        self.impl_of.get(&func).copied()
    }

    /// The traits type parameter `param` has to implement.
    pub fn bounds(&self, param: DefId) -> &[DefId] {
        self.bounds.get(&param).map(Vec::as_slice).unwrap_or(&[])
    }

    /// For `T::Item` or `Self::Item`: the `T` or `Self`.
    pub fn projection_base(&self, ty: Id<TypeExpr>) -> Option<DefId> {
        self.projection_bases.get(&ty).copied()
    }

    /// The standard library's trait the language uses as `lang`, if it's loaded.
    pub fn lang(&self, lang: LangTrait) -> Option<DefId> {
        self.lang.get(&lang).copied()
    }

    /// Which lang trait `def` is, if any.
    pub fn lang_of(&self, def: DefId) -> Option<LangTrait> {
        self.lang.iter().find(|&(_, &d)| d == def).map(|(&l, _)| l)
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

    /// The definition built-in type `primitive`'s own methods are attached to, if an
    /// `impl` gives it any.
    pub fn primitive(&self, primitive: som_ast::Primitive) -> Option<DefId> {
        self.primitives.get(&primitive).copied()
    }

    /// The definition of the built-in type called `name` (`i32`), if it has methods.
    pub fn primitives_named(&self, name: &str) -> Option<DefId> {
        (self.primitives.iter())
            .find(|(p, _)| format!("{p:?}").to_lowercase() == name)
            .map(|(_, &d)| d)
    }

    /// The definition for built-in type `primitive`'s methods, made the first time.
    pub(crate) fn primitive_or_define(
        &mut self,
        primitive: som_ast::Primitive,
        span: som_base::Span,
        module: usize,
    ) -> DefId {
        if let Some(&def) = self.primitives.get(&primitive) {
            return def;
        }
        let name = format!("{primitive:?}").to_lowercase();
        let def = self.define(&name, DefKind::Primitive(primitive), span, module);
        self.primitives.insert(primitive, def);
        def
    }
}

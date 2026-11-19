use som_ast::{Ast, Expr, Func, Id, Pattern, Span, Stmt, Struct, Trait, TypeExpr};
use som_base::Arena;
use som_lang::{LangFunc, LangStruct, LangTrait};
use std::collections::HashMap;

/// A definition: anything a name can refer to.
pub type DefId = Id<Def>;

#[derive(Clone, Debug)]
pub enum DefKind {
    Func(Id<Func>),
    /// A built-in type with methods of its own: `i32` in `impl i32 { … }`.
    Primitive(som_ast::Primitive),
    /// A struct, or an enum.
    Struct(Id<Struct>),
    /// The `index`th variant of enum `strukt`: `Shape::Circle`.
    Variant {
        strukt: Id<Struct>,
        index: usize,
    },
    /// A name a `match` arm's pattern binds: `r` in `Circle(r) => …`.
    Binding(Id<Pattern>),
    Trait(Id<Trait>),
    /// `type Vector3 = Vector<3, f32>;`: another name for the type `ty` describes.
    Alias {
        ty: Id<TypeExpr>,
    },
    /// The `index`th generic parameter of a type alias: `T` in `type Vector3<T> = Vector<3, T>`.
    AliasParam {
        index: usize,
        value: bool,
    },
    /// The `index`th associated type of a trait: `Item` in `trait Iterator { type Item; }`.
    AssocType {
        trait_: Id<Trait>,
        index: usize,
    },
    /// The `index`th generic parameter of impl block `impl_` (see `Names::impls`), as its
    /// target and associated types are written: `T` in `impl<T> Iterator for ListIntoIter<T>`.
    ImplParam {
        impl_: usize,
        index: usize,
        value: bool,
    },
    /// A trait's own `Self` (`index` 0) or `index - 1`th generic parameter, as its
    /// parameters' defaults name them: `Self` in `trait Add<Other = Self>`.
    TraitParam {
        trait_: Id<Trait>,
        index: usize,
    },
    /// `Self` inside impl block `impl_`: its target type.
    SelfType {
        impl_: usize,
    },
    /// The `index`th associated type impl block `impl_` says: `Item` in `type Item = &T;`.
    ImplType {
        impl_: usize,
        index: usize,
    },
    Param {
        func: Id<Func>,
        index: usize,
    },
    Local(Id<Stmt>),
    /// `@const W = 60;`: a value known while compiling.
    Const(Id<som_ast::Const>),
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

impl DefKind {
    /// Whether this names a type rather than a value or a trait.
    pub(crate) fn is_type(&self) -> bool {
        matches!(
            self,
            DefKind::Struct(_)
                | DefKind::TypeParam { .. }
                | DefKind::TraitParam { .. }
                | DefKind::StructParam { .. }
                | DefKind::AssocType { .. }
                | DefKind::ImplParam { .. }
                | DefKind::SelfType { .. }
                | DefKind::ImplType { .. }
                | DefKind::Alias { .. }
                | DefKind::AliasParam { .. }
        )
    }
}

#[derive(Clone, Debug)]
pub struct Def {
    pub name: Box<str>,
    pub kind: DefKind,
    pub span: Span,
    /// Index into `Ast::modules` of the module this is defined in.
    pub module: usize,
}

/// An `impl` block, by its index in `Names::impls`.
pub struct ImplDef {
    pub module: usize,
    pub trait_: Option<DefId>,
    pub owner: Option<DefId>,
    /// Its generic parameters, as its target and associated types use them: `ImplParam`s.
    /// Each method has its own copies, first among its generics.
    pub params: Vec<DefId>,
    pub methods: Vec<Id<Func>>,
    /// The associated types it says, `ImplType`s, in the order written.
    pub types: Vec<DefId>,
    /// `impl<C: Index> IterRef for C`: the parameter it's for, any type meeting its bounds.
    /// A type's own implementation of the trait comes before it.
    pub blanket: Option<DefId>,
}

/// The traits, structs and functions the language uses, where the standard library defines them.
#[derive(Default)]
struct LangItems {
    traits: HashMap<LangTrait, DefId>,
    structs: HashMap<LangStruct, DefId>,
    funcs: HashMap<LangFunc, DefId>,
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
    funcs: Vec<Option<DefId>>,
    params: Vec<Vec<DefId>>,
    type_params: Vec<Vec<DefId>>,
    aliases: Vec<DefId>,
    consts: Vec<Option<DefId>>,
    /// Constants in impl blocks, by the type they're for and their name: `f32::PI`.
    assoc_consts: HashMap<(DefId, Box<str>), DefId>,
    /// The type each constant in an impl block is for.
    const_owners: HashMap<Id<som_ast::Const>, DefId>,
    alias_params: HashMap<DefId, Vec<DefId>>,
    /// `Vector3::zero()`, `Vector3 { … }`: expressions whose path names a struct through an
    /// alias → the alias, whose type the struct's value has.
    expr_aliases: HashMap<Id<Expr>, DefId>,
    struct_params: Vec<Vec<DefId>>,
    structs: Vec<Option<DefId>>,
    /// `(struct, method name)` → the method, for `impl` blocks.
    methods: HashMap<(DefId, Box<str>), DefId>,
    /// `(struct, property name)` → the properties, with the impl block each is in: methods
    /// read without parentheses, named apart from methods. Impl blocks for different types of
    /// a generic struct can each have one, `x` of `Vector<2, T>` and of `Vector<3, T>`; the
    /// type checker picks the one whose block the value's type is for.
    props: HashMap<(DefId, Box<str>), Vec<(usize, DefId)>>,
    variants: Vec<Vec<DefId>>,
    /// Functions sharing a name in a module: the first one's definition → all of them, in
    /// order. A call names the first; the type checker picks the one that fits.
    overloads: HashMap<DefId, Vec<DefId>>,
    traits: Vec<Option<DefId>>,
    trait_types: Vec<Vec<DefId>>,
    /// Each trait's `Self`, then its generic parameters: what its defaults are written in.
    trait_params: Vec<Vec<DefId>>,
    /// `trait IndexMut: Index` → the traits it builds on.
    supers: Vec<Vec<DefId>>,
    /// Every `impl` block, in the order they're written, module by module.
    pub impls: Vec<ImplDef>,
    impl_of: HashMap<Id<Func>, usize>,
    bounds: HashMap<DefId, Vec<DefId>>,
    /// `P: Call<&I::Item, bool>` → the arguments given trait `Call`, for `P`.
    bound_args: HashMap<(DefId, DefId), Vec<Id<TypeExpr>>>,
    /// `F: fn(&T) -> bool` → how an `F` is called, for `F`.
    signatures: HashMap<DefId, Id<TypeExpr>>,
    /// `T::Item`, `Self::Item` → the type parameter or `Self` it's of.
    projection_bases: HashMap<Id<TypeExpr>, DefId>,
    lang: LangItems,
    /// The functions called when the program fails (see `fail_listeners`).
    fail_listeners: Vec<Id<Func>>,
    /// The `@test` functions, in the order they're written.
    tests: Vec<Id<Func>>,
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
            aliases: Vec::new(),
            consts: vec![None; ast.consts.len()],
            assoc_consts: HashMap::new(),
            const_owners: HashMap::new(),
            alias_params: HashMap::new(),
            expr_aliases: HashMap::new(),
            struct_params: vec![Vec::new(); ast.structs.len()],
            structs: vec![None; ast.structs.len()],
            methods: HashMap::new(),
            props: HashMap::new(),
            variants: vec![Vec::new(); ast.structs.len()],
            overloads: HashMap::new(),
            traits: vec![None; ast.traits.len()],
            trait_types: vec![Vec::new(); ast.traits.len()],
            trait_params: vec![Vec::new(); ast.traits.len()],
            supers: vec![Vec::new(); ast.traits.len()],
            impls: Vec::new(),
            impl_of: HashMap::new(),
            bounds: HashMap::new(),
            bound_args: HashMap::new(),
            signatures: HashMap::new(),
            projection_bases: HashMap::new(),
            lang: LangItems::default(),
            fail_listeners: Vec::new(),
            tests: Vec::new(),
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

    pub(crate) fn set_bound_args(&mut self, param: DefId, trait_: DefId, args: Vec<Id<TypeExpr>>) {
        self.bound_args.insert((param, trait_), args);
    }

    pub(crate) fn set_signature(&mut self, param: DefId, signature: Id<TypeExpr>) {
        self.signatures.insert(param, signature);
    }

    pub(crate) fn set_projection_base(&mut self, ty: Id<TypeExpr>, base: DefId) {
        self.projection_bases.insert(ty, base);
    }

    pub(crate) fn set_lang(&mut self, lang: LangTrait, def: DefId) {
        self.lang.traits.insert(lang, def);
    }

    pub(crate) fn set_lang_struct(&mut self, lang: LangStruct, def: DefId) {
        self.lang.structs.insert(lang, def);
    }

    pub(crate) fn set_lang_func(&mut self, lang: LangFunc, def: DefId) {
        self.lang.funcs.insert(lang, def);
    }

    pub(crate) fn set_fail_listeners(&mut self, listeners: Vec<Id<Func>>) {
        self.fail_listeners = listeners;
    }

    pub(crate) fn set_tests(&mut self, tests: Vec<Id<Func>>) {
        self.tests = tests;
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

    pub(crate) fn add_alias(&mut self, def: DefId, params: Vec<DefId>) {
        self.aliases.push(def);
        self.alias_params.insert(def, params);
    }

    pub(crate) fn set_const(&mut self, c: Id<som_ast::Const>, def: DefId) {
        self.consts[c.id] = Some(def);
    }

    /// Adds constant `c`, `def`, to the type `owner`; the one already called that, if there is.
    pub(crate) fn add_assoc_const(
        &mut self,
        owner: DefId,
        c: Id<som_ast::Const>,
        name: &str,
        def: DefId,
    ) -> Option<DefId> {
        self.const_owners.insert(c, owner);
        let key = (owner, name.into());
        if let Some(&previous) = self.assoc_consts.get(&key) {
            return Some(previous);
        }
        self.assoc_consts.insert(key, def);
        None
    }

    /// The constant `owner` has called `name`: `PI` of `f32`.
    pub fn assoc_const(&self, owner: DefId, name: &str) -> Option<DefId> {
        self.assoc_consts.get(&(owner, name.into())).copied()
    }

    pub fn assoc_consts_of(&self, owner: DefId) -> Vec<(&str, DefId)> {
        let mut consts: Vec<(&str, DefId)> = (self.assoc_consts.iter())
            .filter(|((o, _), _)| *o == owner)
            .map(|((_, name), &def)| (&**name, def))
            .collect();
        consts.sort();
        consts
    }

    /// The type constant `c` is in an impl block for, if it is.
    pub fn const_owner(&self, c: Id<som_ast::Const>) -> Option<DefId> {
        self.const_owners.get(&c).copied()
    }

    pub fn const_def(&self, c: Id<som_ast::Const>) -> DefId {
        self.consts[c.id].expect("every constant is defined")
    }

    pub fn aliases(&self) -> &[DefId] {
        &self.aliases
    }

    pub fn alias_params(&self, alias: DefId) -> &[DefId] {
        self.alias_params.get(&alias).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn set_expr_alias(&mut self, expr: Id<Expr>, alias: DefId) {
        self.expr_aliases.insert(expr, alias);
    }

    /// The alias an expression's path names its struct through: `Vector3` in
    /// `Vector3::zero()` or `Vector3 { … }`.
    pub fn expr_alias(&self, expr: Id<Expr>) -> Option<DefId> {
        self.expr_aliases.get(&expr).copied()
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

    /// Adds property `prop`, in impl block `block`. Returns the existing one if that block
    /// already has one with this name.
    pub(crate) fn add_prop(
        &mut self,
        owner: DefId,
        name: &str,
        block: usize,
        prop: DefId,
        conditional: impl Fn(&DefKind) -> bool,
    ) -> Option<DefId> {
        let defs = &self.defs;
        let conditional = |def: DefId| conditional(&defs[def].kind);
        let props = self.props.entry((owner, name.into())).or_default();
        // Two in one block, unless each is only there for some of its types (`@if N >= 3 { … }
        // else { … }`): which is there is worked out where it's used.
        if let Some(&(_, previous)) = (props.iter())
            .find(|&&(b, other)| b == block && !(conditional(other) && conditional(prop)))
        {
            return Some(previous);
        }
        props.push((block, prop));
        None
    }

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
                | DefKind::AliasParam { value: true, .. }
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

    pub fn local(&self, stmt: Id<Stmt>) -> Option<DefId> {
        self.lets[stmt.id]
    }

    pub(crate) fn set_loop_target(&mut self, stmt: Id<Stmt>, def: DefId) {
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

    pub fn trait_def(&self, t: Id<Trait>) -> DefId {
        self.traits[t.id].expect("every trait is defined")
    }

    pub fn trait_types(&self, t: Id<Trait>) -> &[DefId] {
        &self.trait_types[t.id]
    }

    /// Trait `t`'s `Self`, then its generic parameters, as its parameters' defaults name
    /// them.
    pub fn trait_params(&self, t: Id<Trait>) -> &[DefId] {
        &self.trait_params[t.id]
    }

    pub(crate) fn set_trait_params(&mut self, t: Id<Trait>, params: Vec<DefId>) {
        self.trait_params[t.id] = params;
    }

    /// The traits trait `t` builds on: `Index` for `trait IndexMut: Index`.
    pub fn supers(&self, t: Id<Trait>) -> &[DefId] {
        &self.supers[t.id]
    }

    pub(crate) fn set_supers(&mut self, t: Id<Trait>, supers: Vec<DefId>) {
        self.supers[t.id] = supers;
    }

    pub fn impl_of(&self, func: Id<Func>) -> Option<usize> {
        self.impl_of.get(&func).copied()
    }

    pub fn bounds(&self, param: DefId) -> &[DefId] {
        self.bounds.get(&param).map(Vec::as_slice).unwrap_or(&[])
    }

    /// What type parameter `param`'s bound gives trait `trait_`: `&I::Item, bool` for `P:
    /// Call<&I::Item, bool>`. Empty if it gives nothing, leaving the trait's defaults.
    pub fn bound_args(&self, param: DefId, trait_: DefId) -> &[Id<TypeExpr>] {
        (self.bound_args.get(&(param, trait_))).map_or(&[], Vec::as_slice)
    }

    /// How a value of type parameter `param` is called: `fn(&T) -> bool` for `F: fn(&T) ->
    /// bool`.
    pub fn signature(&self, param: DefId) -> Option<Id<TypeExpr>> {
        self.signatures.get(&param).copied()
    }

    /// Every type parameter called some way, in the order defined.
    pub fn called_params(&self) -> Vec<DefId> {
        let mut params: Vec<_> = self.signatures.keys().copied().collect();
        params.sort();
        params
    }

    /// Every type parameter and trait its bound gives arguments, in the order defined.
    pub fn bounds_given(&self) -> Vec<(DefId, DefId)> {
        let mut given: Vec<_> = self.bound_args.keys().copied().collect();
        given.sort();
        given
    }

    /// For `T::Item` or `Self::Item`: the `T` or `Self`.
    pub fn projection_base(&self, ty: Id<TypeExpr>) -> Option<DefId> {
        self.projection_bases.get(&ty).copied()
    }

    /// The standard library's trait the language uses as `lang`, if it's loaded.
    pub fn lang(&self, lang: LangTrait) -> Option<DefId> {
        self.lang.traits.get(&lang).copied()
    }

    /// The standard library's struct the language uses as `lang`, if it's loaded.
    pub fn lang_struct(&self, lang: LangStruct) -> Option<DefId> {
        self.lang.structs.get(&lang).copied()
    }

    pub fn lang_func(&self, lang: LangFunc) -> Option<DefId> {
        self.lang.funcs.get(&lang).copied()
    }

    /// What's called, in order, when the program fails: its own `@on fail` functions, or if
    /// it has none, the standard library's.
    pub fn fail_listeners(&self) -> &[Id<Func>] {
        &self.fail_listeners
    }

    /// Every `@test` function, the standard library's too, in the order they're written.
    pub fn tests(&self) -> &[Id<Func>] {
        &self.tests
    }

    /// The tests `som test` runs: the program's own. The standard library's only when it's
    /// the program, which is loaded as an empty root module in its folder.
    pub fn program_tests(&self, ast: &Ast) -> Vec<Id<Func>> {
        let modules = &ast.modules;
        let std_is_program = (modules.iter())
            .find(|m| *m.name == *som_lang::PRELUDE_MODULE)
            .is_some_and(|std| std.path == modules[0].path);
        (self.tests.iter().copied())
            .filter(|&test| {
                let module = &modules[self.def(self.func(test)).module];
                std_is_program || !som_lang::is_std_module(&module.name)
            })
            .collect()
    }

    /// What the test runner calls `test`: its name, after its module's.
    pub fn test_name(&self, ast: &Ast, test: Id<Func>) -> String {
        let name = &ast.funcs[test].name.name;
        match &*ast.modules[self.def(self.func(test)).module].name {
            "" => name.to_string(),
            module => format!("{module}::{name}"),
        }
    }

    pub fn lang_of(&self, def: DefId) -> Option<LangTrait> {
        self.lang
            .traits
            .iter()
            .find(|&(_, &d)| d == def)
            .map(|(&l, _)| l)
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

    pub fn method(&self, owner: DefId, name: &str) -> Option<DefId> {
        self.methods.get(&(owner, name.into())).copied()
    }

    /// The properties called `name` defined on `owner` by `impl` blocks, one per block.
    pub fn props(&self, owner: DefId, name: &str) -> Vec<DefId> {
        (self.props.get(&(owner, name.into())).into_iter().flatten())
            .map(|&(_, def)| def)
            .collect()
    }

    pub fn props_of(&self, owner: DefId) -> Vec<(&str, DefId)> {
        let mut props: Vec<(&str, DefId)> = (self.props.iter())
            .filter(|((o, _), _)| *o == owner)
            .flat_map(|((_, name), all)| all.iter().map(|&(_, def)| (&**name, def)))
            .collect();
        props.sort();
        props
    }

    /// The definition built-in type `primitive`'s own methods are attached to, if an
    /// `impl` gives it any.
    pub fn primitive(&self, primitive: som_ast::Primitive) -> Option<DefId> {
        self.primitives.get(&primitive).copied()
    }

    /// The definition of the built-in type called `name` (`i32`), if it has methods.
    pub fn primitives_named(&self, name: &str) -> Option<DefId> {
        self.primitives
            .get(&som_ast::Primitive::from_name(name)?)
            .copied()
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
        let def = self.define(
            primitive.name(),
            DefKind::Primitive(primitive),
            span,
            module,
        );
        self.primitives.insert(primitive, def);
        def
    }
}

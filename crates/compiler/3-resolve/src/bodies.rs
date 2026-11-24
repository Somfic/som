//! The second phase: the names in every item's code and types.

use som_ast::{
    BinOp, Const, Expr, Func, Generic, Id, Ident, Impl, Item, Path, Pattern, PatternFields, Stmt,
    Trait, TypeExpr,
};
use std::collections::HashMap;

use crate::names::{DefId, DefKind};
use crate::{Expect, ResolveError, Resolver};

impl<'a> Resolver<'a> {
    pub(crate) fn resolve_bodies(&mut self) {
        let ast = self.ast;
        self.resolve_aliases();
        self.resolve_supers();
        // Impl blocks, counted in the order `collect_methods` saw them.
        let mut impl_index = 0;
        for (m, module) in ast.modules.iter().enumerate() {
            self.cx.module = m;
            for item in &module.items {
                match item {
                    Item::Func(id) => self.resolve_func(*id),
                    Item::Extern(block) => block.funcs.iter().for_each(|&id| self.resolve_func(id)),
                    Item::Impl(block) => {
                        self.resolve_impl(impl_index, block);
                        impl_index += 1;
                    }
                    Item::Trait(id) => self.resolve_trait(*id),
                    Item::Struct(id) => self.resolve_struct(*id),
                    Item::Const(id) => {
                        self.cx.type_params.clear();
                        self.resolve_const(&ast.consts[*id]);
                    }
                    Item::Alias(_) | Item::Use(_) => {}
                }
            }
        }
        self.check_consts();
        self.check_test_calls();
    }

    /// Nothing names a `@test`: the test runner is what calls it.
    fn check_test_calls(&mut self) {
        let ast = self.ast;
        for (id, _) in ast.exprs.iter() {
            let Some(def) = self.names.expr(id) else {
                continue;
            };
            let DefKind::Func(f) = self.names.def(def).kind else {
                continue;
            };
            let func = &ast.funcs[f];
            if func.test {
                self.errors.push(ResolveError::TestCalled {
                    name: func.name.name.to_string(),
                    span: ast.exprs.span(id).clone(),
                    defined_at: func.name.span.clone(),
                });
            }
        }
    }

    /// Its fields can be written in terms of its type parameters.
    fn resolve_struct(&mut self, id: Id<som_ast::Struct>) {
        let strukt = &self.ast.structs[id];
        self.cx.type_params.clear();
        for (index, param) in strukt.generics.iter().enumerate() {
            let kind = DefKind::StructParam {
                strukt: id,
                index,
                value: param.ty.is_some(),
            };
            let def = self.define(&param.name, kind);
            self.names.push_struct_param(id, def);
            self.cx.type_params.push((&param.name.name, def));
        }
        let defs = self.names.struct_params(id).to_vec();
        self.resolve_generics(&strukt.generics, &defs);
        for field in strukt.fields.iter().chain(strukt.payload()) {
            self.resolve_type(field.ty);
        }
        self.cx.type_params.clear();
    }

    /// A constant outside a function: its type and its value.
    fn resolve_const(&mut self, c: &'a Const) {
        if let Some(ty) = c.ty {
            self.resolve_type(ty);
        }
        self.in_scope(|r| r.resolve_expr(c.value));
    }

    /// A constant inside a function: its value is worked out while compiling, so it can't
    /// name what's only there when the function runs.
    fn define_local_const(&mut self, c: Id<Const>) -> DefId {
        let constant = &self.ast.consts[c];
        let outer = self.cx.const_depth.replace(self.cx.scopes.len());
        self.resolve_expr(constant.value);
        self.cx.const_depth = outer;
        let def = self.define(&constant.name, DefKind::Const(c));
        self.names.set_const(c, def);
        def
    }

    /// What each type alias stands for, before any code, which can name a struct through one:
    /// `Vector3::zero()`.
    fn resolve_aliases(&mut self) {
        let ast = self.ast;
        // In the order `collect_items` saw them.
        let mut alias_index = 0;
        for (m, module) in ast.modules.iter().enumerate() {
            self.cx.module = m;
            for item in &module.items {
                let Item::Alias(alias) = item else {
                    continue;
                };
                let def = self.names.aliases()[alias_index];
                alias_index += 1;
                let params = self.names.alias_params(def).to_vec();
                self.cx.type_params.clear();
                for (g, &param) in alias.generics.iter().zip(&params) {
                    self.cx.type_params.push((&g.name.name, param));
                }
                self.resolve_generics(&alias.generics, &params);
                self.resolve_type(alias.ty);
                self.cx.type_params.clear();
            }
        }
    }

    /// An impl block's target, trait and associated types, in terms of its parameters; then
    /// its members, which can name `Self` and the associated types.
    fn resolve_impl(&mut self, index: usize, block: &'a Impl) {
        let params = self.names.impls[index].params.clone();
        self.cx.type_params = (block.generics.iter().zip(&params))
            .map(|(g, &def)| (&*g.name.name, def))
            .collect();
        self.resolve_generics(&block.generics, &params);
        let written = (block.target_args.iter())
            .chain(&block.target_ty)
            .chain(block.trait_.iter().flat_map(|(_, args)| args))
            .chain(block.types.iter().map(|(_, ty)| ty));
        for &ty in written {
            self.resolve_type(ty);
        }
        self.resolve_member_conditions(block);
        self.cx.type_params.clear();

        let self_type = self.names.define(
            som_lang::SELF_TYPE,
            DefKind::SelfType { impl_: index },
            block.target.span(),
            self.cx.module,
        );
        let types = self.names.impls[index].types.clone();
        self.cx.block_types = std::iter::once((som_lang::SELF_TYPE, self_type))
            .chain((block.types.iter().zip(types)).map(|((n, _), d)| (&*n.name, d)))
            .collect();
        let outer = std::mem::replace(&mut self.cx.owner, self.names.impls[index].owner);
        self.resolve_impl_members(block);
        self.cx.owner = outer;
        self.cx.block_types.clear();
    }

    /// `@if N >= 3 { … }` among the members: in terms of the block's parameters, once for
    /// all the members in it.
    fn resolve_member_conditions(&mut self, block: &Impl) {
        let mut conditions: Vec<Id<Expr>> = Vec::new();
        for &method in &block.methods {
            for &condition in &self.ast.funcs[method].when {
                if !conditions.contains(&condition) {
                    conditions.push(condition);
                }
            }
        }
        let outer = std::mem::replace(&mut self.cx.settled_condition, true);
        for condition in conditions {
            self.resolve_expr(condition);
        }
        self.cx.settled_condition = outer;
    }

    /// Its constants are named as they are inside it, `PI` as well as `Self::PI`: by each
    /// other, and by its methods, whose own names come first.
    fn resolve_impl_members(&mut self, block: &'a Impl) {
        let consts = (block.consts.iter())
            .map(|&c| (&*self.ast.consts[c].name.name, self.names.const_def(c)))
            .collect();
        self.cx.scopes.push(consts);
        for &c in &block.consts {
            self.resolve_const(&self.ast.consts[c]);
        }
        block.methods.iter().for_each(|&id| self.resolve_func(id));
        self.pop_scope();
    }

    /// Each trait's supertraits, before anything that could name a type through one:
    /// `C::Item` of a `C: IndexMut` is `Index`'s, wherever the two are written.
    fn resolve_supers(&mut self) {
        let ast = self.ast;
        for (m, module) in ast.modules.iter().enumerate() {
            self.cx.module = m;
            for item in &module.items {
                if let Item::Trait(id) = item {
                    let supers = (ast.traits[*id].supers.iter())
                        .filter_map(|path| self.resolve_path(path, Expect::Trait))
                        .collect();
                    self.names.set_supers(*id, supers);
                }
            }
        }
    }

    /// A trait's methods, which can name its associated types; `Self` is a generic of each.
    fn resolve_trait(&mut self, id: Id<Trait>) {
        let t = &self.ast.traits[id];
        let types = self.names.trait_types(id).to_vec();
        // Its parameters' defaults, `Other = Self`: in terms of `Self` and its parameters.
        let this = Ident {
            name: som_lang::SELF_TYPE.into(),
            span: t.name.span.clone(),
        };
        let params: Vec<DefId> = (std::iter::once(&this).chain(t.generics.iter().map(|g| &g.name)))
            .enumerate()
            .map(|(index, name)| self.define(name, DefKind::TraitParam { trait_: id, index }))
            .collect();
        self.names.set_trait_params(id, params.clone());
        self.cx.type_params = std::iter::once(som_lang::SELF_TYPE)
            .chain(t.generics.iter().map(|g| &*g.name.name))
            .zip(params.iter().copied())
            .collect();
        for g in &t.generics {
            if let Some(default) = g.default {
                self.resolve_type(default);
            }
        }
        self.cx.block_types = (t.types.iter().zip(types))
            .map(|(assoc, def)| (&*assoc.name.name, def))
            .collect();
        // Its aliases, `type Predicate = fn(&Self::Item) -> bool;`: of its own `Self`, which
        // has the trait and those it builds on, so `Self::Item` is the `Item` of those.
        let this = params[0];
        let mut bounds = vec![self.names.trait_def(id)];
        bounds.extend(self.names.supers(id).iter().copied());
        self.names.set_bounds(this, bounds);
        for alias in &t.aliases {
            self.resolve_type(alias.ty);
            let def = self.define(&alias.name, DefKind::Alias { ty: alias.ty });
            self.cx.block_types.push((&alias.name.name, def));
        }
        self.cx.type_params.clear();
        t.methods.iter().for_each(|&m| self.resolve_func(m));
        self.cx.block_types.clear();
    }

    /// Generic parameters `generics`, defined as `defs`: their types, defaults and bounds.
    /// Every bound's trait comes before any bound's arguments, which can name what another
    /// parameter's trait has: `P: Call<&I::Item, bool>`.
    fn resolve_generics(&mut self, generics: &'a [Generic], defs: &[DefId]) {
        let mut given = Vec::new();
        let mut called = Vec::new();
        let mut aliased = Vec::new();
        for (generic, &def) in generics.iter().zip(defs) {
            if let Some(signature) = generic.signature {
                called.push((def, signature));
            }
            if let Some(ty) = generic.ty {
                self.resolve_type(ty);
            }
            if let Some(default) = generic.default {
                self.resolve_type(default);
            }
            let mut traits = Vec::new();
            for bound in &generic.bounds {
                // `P: Predicate`, an alias of a function's type: called that way.
                if let Some(signature) = self.signature_alias(bound) {
                    aliased.push((def, signature));
                    continue;
                }
                let Some(trait_) = self.resolve_path(&bound.trait_, Expect::Trait) else {
                    continue;
                };
                traits.push(trait_);
                if !bound.args.is_empty() {
                    given.push((def, trait_, &bound.args));
                }
            }
            if !traits.is_empty() {
                self.names.set_bounds(def, traits);
            }
        }
        for (def, trait_, args) in given {
            args.iter().for_each(|&arg| self.resolve_type(arg));
            self.names.set_bound_args(def, trait_, args.clone());
        }
        for (def, signature) in called {
            self.resolve_type(signature);
            self.names.set_signature(def, signature);
        }
        // Already resolved where the alias is: in a trait, of its own `Self`.
        for (def, signature) in aliased {
            self.names.set_signature(def, signature);
        }
    }

    /// For a bound naming an alias of a function's type, `P: Predicate`: that type.
    fn signature_alias(&self, bound: &som_ast::Bound) -> Option<Id<TypeExpr>> {
        let [name] = &bound.trait_.segments[..] else {
            return None;
        };
        if !bound.args.is_empty() {
            return None;
        }
        let def = self.lookup(&name.name, Expect::Type)?;
        let DefKind::Alias { ty } = self.names.def(def).kind else {
            return None;
        };
        matches!(self.ast.types[ty], TypeExpr::Fn { .. }).then_some(ty)
    }

    fn resolve_func(&mut self, id: Id<Func>) {
        let func = &self.ast.funcs[id];

        self.cx.type_params.clear();
        for (index, param) in func.generics.iter().enumerate() {
            let kind = DefKind::TypeParam {
                func: id,
                index,
                value: param.ty.is_some(),
            };
            let def = self.define(&param.name, kind);
            self.names.push_type_param(id, def);
            self.cx.type_params.push((&param.name.name, def));
        }
        let defs = self.names.type_params(id).to_vec();
        self.resolve_generics(&func.generics, &defs);

        self.cx.scopes.push(HashMap::new());
        for (index, param) in func.params.iter().enumerate() {
            if let Some(ty) = param.ty {
                self.resolve_type(ty);
            }
            let def = self.define(&param.name, DefKind::Param { func: id, index });
            self.names.push_param(id, def);
            self.bind(&param.name, def);
        }
        if let Some(ty) = func.returns {
            self.resolve_type(ty);
        }
        for clause in &func.requires {
            self.resolve_expr(clause.condition);
        }
        if let Some(body) = func.body {
            self.resolve_expr(body);
        }
        self.pop_scope();
    }

    fn resolve_stmts(&mut self, stmts: &[Id<Stmt>]) {
        self.in_scope(|r| stmts.iter().for_each(|&s| r.resolve_stmt(s)));
    }

    fn resolve_stmt(&mut self, id: Id<Stmt>) {
        match &self.ast.stmts[id] {
            // A module only the rest of the block sees. One that doesn't exist was reported
            // when modules were loaded.
            Stmt::Use(u) => {
                if let Some(&module) = self.modules.by_name.get(u.path.to_string().as_str()) {
                    self.cx.block_imports.push((self.cx.scopes.len(), module));
                }
            }
            Stmt::Const(c) => {
                if let Some(ty) = self.ast.consts[*c].ty {
                    self.resolve_type(ty);
                }
                let def = self.define_local_const(*c);
                self.bind(&self.ast.consts[*c].name, def);
            }
            Stmt::Let {
                name, ty, value, ..
            } => {
                // The value can't see the binding it initialises.
                self.resolve_expr(*value);
                if let Some(ty) = ty {
                    self.resolve_type(*ty);
                }
                let def = self.define(name, DefKind::Local(id));
                self.names.set_let(id, def);
                self.bind(name, def);
            }
            // `@for f in T`, over a generic type: each struct's copy, where it's written.
            Stmt::Expr(_) if let Some(per_type) = self.ast.per_type.get(&id) => {
                self.resolve_copies(per_type.arms.iter().map(|&(_, arm)| arm));
            }
            Stmt::Expr(expr) => self.resolve_expr(*expr),
            Stmt::Loop { body } => self.resolve_stmts(body),
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.resolve_expr(*value);
                }
            }
            Stmt::Break(target) | Stmt::Continue(target) => {
                let keyword = match self.ast.stmts[id] {
                    Stmt::Break(_) => "break",
                    _ => "continue",
                };
                if let Some(name) = target {
                    self.resolve_loop_target(id, keyword, name);
                }
            }
            Stmt::For {
                name,
                iterable,
                body,
                ..
            } => self.resolve_for(id, name, *iterable, body),
            Stmt::While { condition, body } => {
                self.resolve_expr(*condition);
                self.resolve_stmts(body);
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let settled = self.ast.settled.contains(&id);
                let outer = std::mem::replace(&mut self.cx.settled_condition, settled);
                self.resolve_expr(*condition);
                self.cx.settled_condition = outer;
                let taken = self.ast.decided.get(&id).copied();
                if taken != Some(false) {
                    self.resolve_stmts(then);
                }
                if let Some(otherwise) = otherwise
                    && taken != Some(true)
                {
                    self.resolve_stmts(otherwise);
                }
            }
        }
    }

    /// The copies made for each type of a generic one are the same text: what's wrong is
    /// said for the first.
    fn resolve_copies(&mut self, copies: impl Iterator<Item = Id<Expr>>) {
        for (k, copy) in copies.enumerate() {
            let before = self.errors.len();
            self.resolve_expr(copy);
            if k > 0 {
                self.errors.truncate(before);
            }
        }
    }

    /// The loop's name is only bound in its body.
    fn resolve_for(
        &mut self,
        id: Id<Stmt>,
        name: &'a Ident,
        iterable: Id<Expr>,
        body: &[Id<Stmt>],
    ) {
        self.resolve_expr(iterable);
        let def = self.define(name, DefKind::Local(id));
        self.names.set_let(id, def);
        self.in_scope(|r| {
            r.bind(name, def);
            r.resolve_stmts(body);
        });
    }

    /// The `x` in `break x;` or `continue x;`: a variable in scope, which has to be an
    /// enclosing `for` loop's. An inner loop with the same name hides an outer one, as it does
    /// for reading it.
    fn resolve_loop_target(&mut self, stmt: Id<Stmt>, keyword: &'static str, name: &Ident) {
        let Some(def) = self.lookup(&name.name, Expect::Value) else {
            self.errors.push(ResolveError::UnboundVariable {
                name: name.name.to_string(),
                span: name.span.clone(),
                help: None,
                fixes: Vec::new(),
            });
            return;
        };
        let defined = self.names.def(def);
        match defined.kind {
            DefKind::Local(loop_stmt) if matches!(self.ast.stmts[loop_stmt], Stmt::For { .. }) => {
                self.names.set_loop_target(stmt, def);
            }
            _ => self.errors.push(ResolveError::NotALoop {
                keyword,
                name: name.name.to_string(),
                span: name.span.clone(),
                defined_at: defined.span.clone(),
            }),
        }
    }

    pub(crate) fn resolve_expr(&mut self, id: Id<Expr>) {
        match &self.ast.exprs[id] {
            Expr::Fail(message) | Expr::Raise(message) | Expr::Propagate(message) => {
                self.resolve_expr(*message)
            }
            Expr::InPlace(c) => {
                self.define_local_const(*c);
            }
            Expr::Hole
            | Expr::Unreachable
            | Expr::Int(_)
            | Expr::Float(_)
            | Expr::Bool(_)
            | Expr::Str(_)
            | Expr::None => {}
            Expr::Type(ty) => self.resolve_type(*ty),
            Expr::Binary {
                op: BinOp::Equals | BinOp::NotEquals,
                lhs,
                rhs,
            } if self.cx.settled_condition => self.resolve_type_comparison(*lhs, *rhs),
            // `usize::MAX`: an integer type's limit, which isn't defined anywhere.
            Expr::Var(path) if path.int_limit().is_some() => {}
            Expr::Var(path) => self.bind_expr(id, path, Expect::Value),
            // Visible from here to the end of the block, like a `let` before the call.
            Expr::Out(name) => {
                let def = self.define(name, DefKind::Out(id));
                self.names.set_expr(id, def);
                self.bind(name, def);
            }
            Expr::Binary { lhs, rhs, .. }
            | Expr::Fallback {
                optional: lhs,
                fallback: rhs,
            }
            | Expr::Assign {
                target: lhs,
                value: rhs,
            }
            | Expr::Index {
                object: lhs,
                index: rhs,
            } => {
                self.resolve_expr(*lhs);
                self.resolve_expr(*rhs);
            }
            Expr::Not(inner)
            | Expr::Neg(inner)
            | Expr::BitNot(inner)
            | Expr::Deref(inner)
            | Expr::Borrow { expr: inner, .. }
            | Expr::Field { object: inner, .. } => self.resolve_expr(*inner),
            Expr::ArrayRepeat { value, count } => {
                self.resolve_expr(*value);
                self.resolve_type(*count);
            }
            Expr::Array(items) => items.iter().for_each(|&item| self.resolve_expr(item)),
            Expr::Interpolated(parts) => {
                for part in parts {
                    if let som_ast::Part::Value(value) = part {
                        self.resolve_expr(*value);
                    }
                }
            }
            Expr::Range { start, end } => {
                start.iter().chain(end).for_each(|&e| self.resolve_expr(e));
            }
            Expr::Cast { value, ty } => {
                self.resolve_expr(*value);
                self.resolve_type(*ty);
            }
            Expr::Call {
                callee,
                owner_args,
                type_args,
                args,
            } => {
                if !self.foreign_call(id, callee, args) {
                    // `test(x)`, `test` a variable holding a function: called through it.
                    let local = match &callee.segments[..] {
                        [name] => self.local(&name.name),
                        _ => None,
                    };
                    let expect = if local.is_some() {
                        Expect::Value
                    } else {
                        Expect::Func
                    };
                    self.bind_expr(id, callee, expect);
                    (owner_args.iter().chain(type_args)).for_each(|&t| self.resolve_type(t));
                }
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            Expr::CallValue { callee, args } => {
                self.resolve_expr(*callee);
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            Expr::Closure { params, body } => self.resolve_closure(id, params, *body),
            // The method itself depends on the receiver's type, so the type checker finds it.
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                if !self.foreign_print(*receiver, method) {
                    self.resolve_expr(*receiver);
                }
                args.iter().for_each(|&a| self.resolve_expr(a));
            }
            Expr::Construct { ty, args, fields } => {
                self.bind_expr(id, ty, Expect::Struct);
                args.iter().for_each(|&arg| self.resolve_type(arg));
                fields
                    .iter()
                    .for_each(|(_, value)| self.resolve_expr(*value));
            }
            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                self.resolve_expr(*condition);
                self.resolve_expr(*then);
                self.resolve_expr(*otherwise);
            }
            Expr::Block { stmts, value } => self.in_scope(|r| {
                stmts.iter().for_each(|&s| r.resolve_stmt(s));
                if let Some(value) = value {
                    r.resolve_expr(*value);
                }
            }),
            // `@for (i, k) in T` among its arms, over a generic enum: each enum's copy.
            Expr::Match { scrutinee, .. }
                if let Some(per_type) = self.ast.per_type_matches.get(&id) =>
            {
                self.resolve_expr(*scrutinee);
                self.resolve_copies(per_type.arms.iter().map(|&(_, copy)| copy));
            }
            // `else e => …`: `e` is only seen in what comes after the `=>`.
            Expr::Recover {
                failable,
                error,
                body,
            } => {
                self.resolve_expr(*failable);
                self.in_scope(|r| {
                    r.resolve_pattern(*error);
                    r.resolve_expr(*body);
                });
            }
            // What an arm's pattern binds is only seen in that arm.
            Expr::Match { scrutinee, arms } => {
                self.resolve_expr(*scrutinee);
                for arm in arms {
                    self.in_scope(|r| {
                        r.resolve_pattern(arm.pattern);
                        r.resolve_expr(arm.body);
                    });
                }
            }
        }
    }

    /// `@if T == Point`: a type compared with another, where nothing else is named so.
    fn resolve_type_comparison(&mut self, lhs: Id<Expr>, rhs: Id<Expr>) {
        for operand in [lhs, rhs] {
            if let Expr::Var(path) = &self.ast.exprs[operand]
                && let [only] = &path.segments[..]
                && self.lookup(&only.name, Expect::Value).is_none()
                && let Some(def) = self.lookup(&only.name, Expect::Type)
            {
                self.names.set_expr(operand, def);
            } else {
                self.resolve_expr(operand);
            }
        }
    }

    fn resolve_pattern(&mut self, id: Id<Pattern>) {
        match &self.ast.patterns[id] {
            Pattern::Wildcard
            | Pattern::Int(_)
            | Pattern::Bool(_)
            | Pattern::Str(_)
            | Pattern::None => {}
            // `Empty`, if some enum in scope has a variant of that name: the type matched
            // decides whose.
            Pattern::Binding { name, mutable } => {
                if !*mutable && self.is_variant_name(&name.name) {
                    return;
                }
                let def = self.define(name, DefKind::Binding(id));
                self.names.set_pattern(id, def);
                self.bind(name, def);
            }
            // `fail NotFound { path }`: a struct's name, as nothing matched says whose it is.
            Pattern::Failed(inner) => {
                if let Pattern::Variant { path, .. } = &self.ast.patterns[*inner]
                    && path.segments.len() == 1
                    && let Some(def) = self.resolve_path(path, Expect::Type)
                {
                    self.names.set_pattern(*inner, def);
                }
                self.resolve_pattern(*inner);
            }
            Pattern::Variant { path, fields } => {
                // `Shape::Circle(r)`; a bare `Circle(r)` is found from the type matched.
                if path.segments.len() > 1
                    && let Some(def) = self.resolve_path(path, Expect::Value)
                {
                    self.names.set_pattern(id, def);
                }
                // `Point { x, y }`: a struct's fields, if a struct in scope has that name.
                if let [only] = &path.segments[..]
                    && matches!(fields, PatternFields::Named { .. })
                    && self.names.pattern(id).is_none()
                    && let Some(def) = self.lookup(&only.name, Expect::Type)
                    && let DefKind::Struct(s) = self.names.def(def).kind
                    && !self.ast.structs[s].is_enum()
                {
                    self.names.set_pattern(id, def);
                }
                match fields {
                    PatternFields::Unit => {}
                    PatternFields::Tuple(fields) => {
                        fields.iter().for_each(|&f| self.resolve_pattern(f));
                    }
                    PatternFields::Named { fields, .. } => {
                        fields.iter().for_each(|&(_, f)| self.resolve_pattern(f));
                    }
                }
            }
        }
    }

    /// `n => *n > limit`: a scope of its own for its parameters, and what it reads of the
    /// variables around it noted as its captures.
    fn resolve_closure(&mut self, id: Id<Expr>, params: &'a [som_ast::Param], body: Id<Expr>) {
        let span = self.ast.exprs.span(id).clone();
        let module = self.cx.module;
        let def = self.names.define("closure", DefKind::Closure(id), span, module);
        let depth = self.cx.scopes.len();
        self.cx.closures.push((id, depth));
        self.in_scope(|r| {
            let mut defs = Vec::new();
            for (index, param) in params.iter().enumerate() {
                if let Some(ty) = param.ty {
                    r.resolve_type(ty);
                }
                let kind = DefKind::ClosureParam { closure: id, index };
                let param_def = r.define(&param.name, kind);
                r.bind(&param.name, param_def);
                defs.push(param_def);
            }
            r.names.set_closure(id, def, defs);
            r.resolve_expr(body);
        });
        self.cx.closures.pop();
    }

    /// `def`, read here: a capture of each anonymous function being resolved that it's from
    /// outside of.
    fn note_capture(&mut self, def: DefId) {
        if self.cx.closures.is_empty() || !self.names.def(def).kind.is_variable() {
            return;
        }
        let Some(at) = (self.cx.scopes.iter()).rposition(|scope| scope.values().any(|&d| d == def))
        else {
            return;
        };
        for &(closure, depth) in &self.cx.closures {
            if at < depth {
                self.names.add_capture(closure, def);
            }
        }
    }

    fn bind_expr(&mut self, id: Id<Expr>, path: &Path, expect: Expect) {
        self.through_alias.set(None);
        let Some(def) = self.resolve_path(path, expect) else {
            return;
        };
        self.note_capture(def);
        if self.local_outside_const(def) {
            self.errors.push(ResolveError::ConstNotKnown {
                span: self.ast.exprs.span(id).clone(),
            });
        }
        self.names.set_expr(id, def);
        if let Some(alias) = self.through_alias.take() {
            self.names.set_expr_alias(id, alias);
        }
    }

    /// Whether `def` is a local from outside the constant being resolved. A generic
    /// parameter, `N`, is known: it's worked out once for each set of arguments the function
    /// is used with.
    fn local_outside_const(&self, def: DefId) -> bool {
        let Some(depth) = self.cx.const_depth else {
            return false;
        };
        let local = self.names.def(def).kind.is_variable();
        local
            && (self.cx.scopes.iter())
                .rposition(|scope| scope.values().any(|&d| d == def))
                .is_some_and(|at| at < depth)
    }

    fn resolve_type(&mut self, id: Id<TypeExpr>) {
        match &self.ast.types[id] {
            // `T::Item`, `Self::Item`: an associated type of a type parameter's trait.
            TypeExpr::Named { path, args }
                if path.segments.len() == 2
                    && let Some(base) = self.lookup(&path.segments[0].name, Expect::Type)
                    && let Some(assoc) = self.associated(base, &path.segments[1].name) =>
            {
                self.names.set_type(id, assoc);
                if !matches!(self.names.def(assoc).kind, DefKind::ImplType { .. }) {
                    self.names.set_projection_base(id, base);
                }
                args.iter().for_each(|&arg| self.resolve_type(arg));
            }
            TypeExpr::Named { path, args } => {
                if let Some(def) = self.resolve_path(path, Expect::Type) {
                    self.names.set_type(id, def);
                }
                args.iter().for_each(|&arg| self.resolve_type(arg));
            }
            TypeExpr::Dyn { path, args } => {
                if let Some(def) = self.resolve_path(path, Expect::Trait) {
                    self.names.set_type(id, def);
                }
                args.iter().for_each(|&arg| self.resolve_type(arg));
            }
            TypeExpr::Reference { to, .. }
            | TypeExpr::PointerTo(to)
            | TypeExpr::Optional(to)
            | TypeExpr::Shared(to)
            | TypeExpr::Elements(to)
            | TypeExpr::Slice { element: to, .. } => self.resolve_type(*to),
            TypeExpr::Array {
                element: lhs,
                len: rhs,
            }
            | TypeExpr::Binary { lhs, rhs, .. } => {
                self.resolve_type(*lhs);
                self.resolve_type(*rhs);
            }
            TypeExpr::Fn { params, returns } => {
                params.iter().for_each(|&p| self.resolve_type(p));
                if let Some(returns) = returns {
                    self.resolve_type(*returns);
                }
            }
            TypeExpr::Failable { ok, errors } => {
                self.resolve_type(*ok);
                errors.iter().flatten().for_each(|&e| self.resolve_type(e));
            }
            TypeExpr::Unit
            | TypeExpr::Primitive(_)
            | TypeExpr::Pointer
            | TypeExpr::Int(_)
            | TypeExpr::Bool(_) => {}
        }
    }

    /// The associated type `name` of what `base` names: of a trait a type parameter has to
    /// implement, or the one an impl block says, for its `Self`.
    fn associated(&self, base: DefId, name: &str) -> Option<DefId> {
        let named =
            |defs: &[DefId]| (defs.iter().copied()).find(|&d| &*self.names.def(d).name == name);
        match self.names.def(base).kind {
            DefKind::SelfType { impl_ } => named(&self.names.impls[impl_].types),
            DefKind::TypeParam { .. }
            | DefKind::StructParam { .. }
            | DefKind::ImplParam { .. }
            | DefKind::TraitParam { .. } => {
                // Its traits', then theirs' supertraits'.
                let mut traits = self.names.bounds(base).to_vec();
                let mut seen = Vec::new();
                while let Some(t) = traits.pop() {
                    let DefKind::Trait(t) = self.names.def(t).kind else {
                        continue;
                    };
                    if seen.contains(&t) {
                        continue;
                    }
                    seen.push(t);
                    if let Some(found) = named(self.names.trait_types(t)) {
                        return Some(found);
                    }
                    traits.extend(self.names.supers(t));
                }
                None
            }
            _ => None,
        }
    }
}

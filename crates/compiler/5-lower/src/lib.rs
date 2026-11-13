//! Lowers the typed syntax tree to MIR.
//!
//! - Expressions become sequences of assignments to locals and temporaries,
//!   evaluated left to right.
//! - `if`, `while`, `loop` and conditional expressions become branches between blocks.
//! - Each block scope ends with `StorageDead` for its `let`s; each function ends with
//!   `StorageDead` for its parameters.
//! - Generic functions are monomorphized: every distinct set of type arguments a
//!   generic function is called with becomes its own MIR function.

use som_ast::{Ast, Expr, Func, Id, Item, Span, Stmt};
use som_mir::*;
use som_resolve::{DefId, DefKind, Names};
use som_typeck::Typed;
use std::collections::HashMap;

pub fn lower(ast: &Ast, names: &Names, typed: &Typed) -> Mir {
    let mut lowerer = Lowerer {
        ast,
        names,
        typed,
        mir: Mir::new(typed.table.clone()),
        instances: HashMap::new(),
        queue: Vec::new(),
        owners: HashMap::new(),
    };
    lowerer.run();
    lowerer.mir
}

struct Lowerer<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    mir: Mir,
    /// `(function, type arguments)` → its MIR function.
    instances: HashMap<(Id<Func>, Vec<TypeId>), FuncId>,
    /// Instances whose bodies still need lowering.
    queue: Vec<(FuncId, Id<Func>, Vec<TypeId>)>,
    /// Method → the name of the struct its `impl` block is for.
    owners: HashMap<Id<Func>, String>,
}

impl<'a> Lowerer<'a> {
    fn run(&mut self) {
        let ast = self.ast;
        for module in &ast.modules {
            for item in &module.items {
                if let Item::Impl(block) = item {
                    for &method in &block.methods {
                        self.owners.insert(method, block.target.name.to_string());
                    }
                }
            }
        }

        for (id, s) in ast.structs.iter() {
            let def = self.names.strukt(id);
            let fields = s
                .fields
                .iter()
                .zip(self.typed.fields(id))
                .map(|(f, &ty)| (f.name.name.to_string(), ty))
                .collect();
            let name = self.qualified(def, &s.name.name);
            self.mir.add_struct(StructDef {
                def,
                name,
                fields,
                copy: s.copy,
            });
        }

        // Every non-generic function, whether or not anything calls it. Generic ones are
        // added as they're called.
        for (id, func) in ast.funcs.iter() {
            let is_alias =
                !matches!(self.names.def(self.names.func(id)).kind, DefKind::Func(f) if f == id);
            if func.type_params.is_empty() && !is_alias {
                self.instance(id, Vec::new());
            }
        }

        while let Some((func_id, func, type_args)) = self.queue.pop() {
            let body = FnBuilder::new(self, func, type_args).build();
            self.mir.funcs[func_id.0 as usize].body = Some(body);
        }
    }

    /// `name` prefixed with its module, if it isn't in the root module.
    fn qualified(&self, def: DefId, name: &str) -> String {
        let module = &self.ast.modules[self.names.def(def).module].name;
        if module.is_empty() {
            name.to_string()
        } else {
            format!("{module}::{name}")
        }
    }

    /// The MIR function for `func` instantiated with `type_args`, creating it if needed.
    fn instance(&mut self, func: Id<Func>, type_args: Vec<TypeId>) -> FuncId {
        let key = (func, type_args);
        if let Some(&id) = self.instances.get(&key) {
            return id;
        }
        let (_, type_args) = &key;
        let ast_func = &self.ast.funcs[func];
        let def = self.names.func(func);

        let subst = Subst::new(self.names.type_params(func), type_args);
        let sig = self.typed.func(func);
        let params = sig
            .params
            .iter()
            .map(|&p| subst.apply(&mut self.mir.types, p))
            .collect();
        let ret = subst.apply(&mut self.mir.types, sig.ret);

        let mut name = ast_func.name.name.to_string();
        if let Some(owner) = self.owners.get(&func) {
            name = format!("{owner}::{name}");
        }
        name = self.qualified(def, &name);
        if !type_args.is_empty() {
            let args: Vec<String> = type_args.iter().map(|&t| self.mir.display_ty(t)).collect();
            name = format!("{name}<{}>", args.join(", "));
        }

        let is_entry = &*ast_func.name.name == "main"
            && self.ast.modules[self.names.def(def).module].name.is_empty()
            && !self.owners.contains_key(&func)
            && type_args.is_empty();

        let id = FuncId(self.mir.funcs.len() as u32);
        self.mir.funcs.push(Function {
            name,
            params,
            ret,
            span: self.ast.funcs.span(func).clone(),
            body: None,
            link_name: ast_func.body.is_none().then(|| {
                ast_func
                    .link_name
                    .as_deref()
                    .unwrap_or(&ast_func.name.name)
                    .to_string()
            }),
            is_entry,
            variadic: ast_func.variadic,
        });
        if ast_func.body.is_some() {
            self.queue.push((id, func, type_args.clone()));
        }
        self.instances.insert(key, id);
        id
    }
}

/// Replaces a generic function's type parameters with concrete types.
struct Subst {
    map: Vec<(DefId, TypeId)>,
}

impl Subst {
    fn new(params: &[DefId], args: &[TypeId]) -> Subst {
        Subst {
            map: params.iter().copied().zip(args.iter().copied()).collect(),
        }
    }

    fn apply(&self, types: &mut TypeTable, ty: TypeId) -> TypeId {
        if self.map.is_empty() {
            return ty;
        }
        match types.get(ty).clone() {
            Ty::Param(def) => self
                .map
                .iter()
                .find(|(d, _)| *d == def)
                .map_or(ty, |(_, t)| *t),
            Ty::Ref { mutable, to } => {
                let to = self.apply(types, to);
                types.intern(Ty::Ref { mutable, to })
            }
            Ty::Fn { params, ret } => {
                let params = params.iter().map(|&p| self.apply(types, p)).collect();
                let ret = self.apply(types, ret);
                types.intern(Ty::Fn { params, ret })
            }
            _ => ty,
        }
    }
}

/// Builds the body of one function instance.
struct FnBuilder<'l, 'a> {
    l: &'l mut Lowerer<'a>,
    func: Id<Func>,
    subst: Subst,
    locals: Vec<LocalDecl>,
    blocks: Vec<(Vec<Statement>, Option<Terminator>)>,
    current: BlockId,
    /// Parameters and `let` bindings → their locals.
    bindings: HashMap<DefId, Local>,
    /// The `let` locals of each enclosing scope, innermost last.
    scopes: Vec<Vec<Local>>,
}

impl<'l, 'a> FnBuilder<'l, 'a> {
    fn new(l: &'l mut Lowerer<'a>, func: Id<Func>, type_args: Vec<TypeId>) -> Self {
        let subst = Subst::new(l.names.type_params(func), &type_args);
        FnBuilder {
            l,
            func,
            subst,
            locals: Vec::new(),
            blocks: vec![(Vec::new(), None)],
            current: BlockId(0),
            bindings: HashMap::new(),
            scopes: Vec::new(),
        }
    }

    fn build(mut self) -> Body {
        let ast = self.l.ast;
        let func = &ast.funcs[self.func];
        let span = ast.funcs.span(self.func).clone();
        let sig = self.l.typed.func(self.func);

        let ret = self.ty(sig.ret);
        self.new_local(ret, None, false, span.clone());
        let params: Vec<TypeId> = sig.params.clone();
        for ((param, &def), ty) in func
            .params
            .iter()
            .zip(self.l.names.params(self.func))
            .zip(params)
        {
            // There's no syntax for `mut` parameters yet, so all parameters are assignable.
            let ty = self.ty(ty);
            let local = self.new_local(
                ty,
                Some(param.name.name.to_string()),
                true,
                param.name.span.clone(),
            );
            self.bindings.insert(def, local);
        }

        let body = func.body.expect("only functions with bodies are lowered");
        let value = self.operand(body);
        self.assign(
            Place::local(Local::RETURN),
            Rvalue::Use(value),
            span.clone(),
        );
        for arg in 1..=func.params.len() as u32 {
            self.push(StatementKind::StorageDead(Local(arg)), span.clone());
        }
        self.terminate(TerminatorKind::Return, span.clone());

        let blocks = self
            .blocks
            .into_iter()
            .map(|(stmts, terminator)| Block {
                stmts,
                // Blocks nothing jumps to, like the one after an endless `loop`, end here.
                terminator: terminator.unwrap_or(Terminator {
                    kind: TerminatorKind::Unreachable,
                    span: span.clone(),
                }),
            })
            .collect();
        let outs = (func.params.iter().enumerate())
            .filter(|(_, p)| p.out)
            .map(|(i, _)| Local(i as u32 + 1))
            .collect();
        Body {
            locals: self.locals,
            arg_count: func.params.len(),
            outs,
            blocks,
        }
    }

    // --- Building blocks ---

    fn ty(&mut self, ty: TypeId) -> TypeId {
        self.subst.apply(&mut self.l.mir.types, ty)
    }

    fn expr_ty(&mut self, expr: Id<Expr>) -> TypeId {
        let ty = self.l.typed.expr(expr);
        self.ty(ty)
    }

    fn span(&self, expr: Id<Expr>) -> Span {
        self.l.ast.exprs.span(expr).clone()
    }

    fn new_local(&mut self, ty: TypeId, name: Option<String>, mutable: bool, span: Span) -> Local {
        let local = Local(self.locals.len() as u32);
        self.locals.push(LocalDecl {
            ty,
            name,
            mutable,
            span,
        });
        local
    }

    fn temp(&mut self, ty: TypeId, span: Span) -> Local {
        self.new_local(ty, None, false, span)
    }

    fn new_block(&mut self) -> BlockId {
        self.blocks.push((Vec::new(), None));
        BlockId(self.blocks.len() as u32 - 1)
    }

    fn push(&mut self, kind: StatementKind, span: Span) {
        self.blocks[self.current.0 as usize]
            .0
            .push(Statement { kind, span });
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue, span: Span) {
        self.push(StatementKind::Assign(place, rvalue), span);
    }

    fn terminate(&mut self, kind: TerminatorKind, span: Span) {
        let block = &mut self.blocks[self.current.0 as usize];
        debug_assert!(block.1.is_none(), "block terminated twice");
        block.1 = Some(Terminator { kind, span });
    }

    /// Reads a place, copying or moving depending on its type.
    fn read(&self, place: Place, ty: TypeId) -> Operand {
        if self.l.mir.is_copy(ty) {
            Operand::Copy(place)
        } else {
            Operand::Move(place)
        }
    }

    fn constant(&self, kind: ConstKind, ty: TypeId) -> Operand {
        Operand::Const(Const { kind, ty })
    }

    fn in_scope<T>(&mut self, span: Span, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.push(Vec::new());
        let result = f(self);
        let locals = self.scopes.pop().expect("scope was pushed");
        for local in locals.into_iter().rev() {
            self.push(StatementKind::StorageDead(local), span.clone());
        }
        result
    }

    // --- Statements ---

    fn stmts(&mut self, stmts: &[Id<Stmt>], span: Span) {
        self.in_scope(span, |b| stmts.iter().for_each(|&s| b.stmt(s)));
    }

    fn stmt(&mut self, id: Id<Stmt>) {
        let ast = self.l.ast;
        let span = ast.stmts.span(id).clone();
        match &ast.stmts[id] {
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => {
                let value = self.rvalue(*value);
                let def = self.l.names.local(id).expect("every let is resolved");
                let ty = self.l.typed.local(def);
                let ty = self.ty(ty);
                let local =
                    self.new_local(ty, Some(name.name.to_string()), *mutable, name.span.clone());
                self.bindings.insert(def, local);
                self.scopes
                    .last_mut()
                    .expect("lets are inside a scope")
                    .push(local);
                self.push(StatementKind::StorageLive(local), span.clone());
                self.assign(Place::local(local), value, span);
            }
            Stmt::Expr(expr) => {
                // The result is unused: a temporary holding it dies with the statement.
                if let Operand::Copy(place) | Operand::Move(place) = self.operand(*expr)
                    && place.projection.is_empty()
                    && place.local != Local::RETURN
                    && self.locals[place.local.0 as usize].name.is_none()
                {
                    self.push(StatementKind::StorageDead(place.local), span);
                }
            }
            Stmt::Loop { body } => {
                let start = self.new_block();
                self.terminate(TerminatorKind::Goto(start), span.clone());
                self.current = start;
                self.stmts(body, span.clone());
                self.terminate(TerminatorKind::Goto(start), span);
                // There's no `break` yet, so nothing follows a `loop`.
                self.current = self.new_block();
            }
            Stmt::While { condition, body } => {
                let (header, body_block, exit) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.terminate(TerminatorKind::Goto(header), span.clone());
                self.current = header;
                let condition = self.operand(*condition);
                self.terminate(
                    TerminatorKind::Branch {
                        condition,
                        then: body_block,
                        otherwise: exit,
                    },
                    span.clone(),
                );
                self.current = body_block;
                self.stmts(body, span.clone());
                self.terminate(TerminatorKind::Goto(header), span);
                self.current = exit;
            }
            Stmt::If {
                condition,
                then,
                otherwise,
            } => {
                let condition = self.operand(*condition);
                let (then_block, else_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.terminate(
                    TerminatorKind::Branch {
                        condition,
                        then: then_block,
                        otherwise: else_block,
                    },
                    span.clone(),
                );
                self.current = then_block;
                self.stmts(then, span.clone());
                self.terminate(TerminatorKind::Goto(join), span.clone());
                self.current = else_block;
                if let Some(otherwise) = otherwise {
                    self.stmts(otherwise, span.clone());
                }
                self.terminate(TerminatorKind::Goto(join), span);
                self.current = join;
            }
        }
    }

    // --- Expressions ---

    /// Evaluates `expr` to an operand: a constant or a read of a place.
    fn operand(&mut self, expr: Id<Expr>) -> Operand {
        let ast = self.l.ast;
        let ty = self.expr_ty(expr);
        let span = self.span(expr);

        match &ast.exprs[expr] {
            Expr::Int(n) => self.constant(ConstKind::Int(*n), ty),
            Expr::Float(x) => self.constant(ConstKind::Float(*x), ty),
            Expr::Bool(b) => self.constant(ConstKind::Bool(*b), ty),
            Expr::Str(s) => self.constant(ConstKind::Str(s.to_string()), ty),
            Expr::Hole => unreachable!("programs with parse errors are never lowered"),

            Expr::Var(_) => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                match self.l.names.def(def).kind {
                    DefKind::Func(func) => {
                        let type_args = self.type_args(expr);
                        let id = self.l.instance(func, type_args);
                        self.constant(ConstKind::Func(id), ty)
                    }
                    _ => {
                        let place = self.variable(def).expect("variables are bound");
                        self.read(place, ty)
                    }
                }
            }

            Expr::Field { .. } | Expr::Deref(_) => {
                let place = self.place(expr);
                self.read(place, ty)
            }

            Expr::Assign { target, value } => {
                let value = self.operand(*value);
                let place = self.place(*target);
                self.assign(place, Rvalue::Use(value), span);
                let unit = self.l.mir.types.unit;
                self.constant(ConstKind::Unit, unit)
            }

            Expr::Block { stmts, value } => self.in_scope(span.clone(), |b| {
                stmts.iter().for_each(|&s| b.stmt(s));
                let result = match value {
                    Some(value) => b.operand(*value),
                    None => b.constant(ConstKind::Unit, ty),
                };
                // Locals die at the end of this scope, so take the value out of them first.
                let declares_locals = !b.scopes.last().expect("in scope").is_empty();
                match result {
                    Operand::Copy(_) | Operand::Move(_) if declares_locals => {
                        let temp = b.temp(ty, span.clone());
                        b.assign(Place::local(temp), Rvalue::Use(result), span.clone());
                        b.read(Place::local(temp), ty)
                    }
                    result => result,
                }
            }),

            Expr::Conditional {
                condition,
                then,
                otherwise,
            } => {
                let result = self.temp(ty, span.clone());
                let condition = self.operand(*condition);
                let (then_block, else_block, join) =
                    (self.new_block(), self.new_block(), self.new_block());
                self.terminate(
                    TerminatorKind::Branch {
                        condition,
                        then: then_block,
                        otherwise: else_block,
                    },
                    span.clone(),
                );
                for (block, branch) in [(then_block, *then), (else_block, *otherwise)] {
                    self.current = block;
                    let value = self.operand(branch);
                    self.assign(Place::local(result), Rvalue::Use(value), span.clone());
                    self.terminate(TerminatorKind::Goto(join), span.clone());
                }
                self.current = join;
                self.read(Place::local(result), ty)
            }

            _ => {
                let rvalue = self.rvalue(expr);
                let temp = self.temp(ty, span.clone());
                self.assign(Place::local(temp), rvalue, span);
                self.read(Place::local(temp), ty)
            }
        }
    }

    /// Evaluates `expr` to a value that can be assigned to a place.
    fn rvalue(&mut self, expr: Id<Expr>) -> Rvalue {
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Binary { op, lhs, rhs } => Rvalue::Binary {
                op: *op,
                lhs: self.operand(*lhs),
                rhs: self.operand(*rhs),
            },
            Expr::Not(inner) => Rvalue::Not(self.operand(*inner)),
            Expr::Borrow {
                mutable,
                expr: inner,
            } => {
                // Make sure the reference type exists in MIR's table, even after substitution.
                self.expr_ty(expr);
                Rvalue::Ref {
                    mutable: *mutable,
                    place: self.place(*inner),
                }
            }
            // A variable living to the end of the surrounding scope, like a `let` before the
            // call. It starts zeroed: C functions don't always fill in their out-parameters.
            Expr::Out(name) => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                let ty = self.ty(self.l.typed.local(def));
                let span = name.span.clone();
                let local = self.new_local(ty, Some(name.name.to_string()), true, span.clone());
                self.bindings.insert(def, local);
                self.scopes
                    .last_mut()
                    .expect("arguments are inside a scope")
                    .push(local);
                self.push(StatementKind::StorageLive(local), span.clone());
                let zero = self.constant(ConstKind::Zeroed, ty);
                self.assign(Place::local(local), Rvalue::Use(zero), span);
                // Make sure the reference type exists in MIR's table, even after substitution.
                self.expr_ty(expr);
                Rvalue::Ref {
                    mutable: true,
                    place: Place::local(local),
                }
            }
            Expr::Construct { fields, .. } => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                let DefKind::Struct(s) = self.l.names.def(def).kind else {
                    unreachable!("constructors name structs")
                };
                let declared = &ast.structs[s].fields;
                // Evaluate in source order, then put the values in declaration order.
                let mut values: Vec<Option<Operand>> = vec![None; declared.len()];
                for (name, value) in fields {
                    let index = declared
                        .iter()
                        .position(|f| f.name.name == name.name)
                        .expect("checked field");
                    values[index] = Some(self.operand(*value));
                }
                let ty = self.expr_ty(expr);
                debug_assert!(self.l.mir.types.lookup(&Ty::Struct(def)) == Some(ty));
                Rvalue::Struct {
                    def,
                    fields: values
                        .into_iter()
                        .map(|v| v.expect("checked: no missing fields"))
                        .collect(),
                }
            }
            Expr::Call { args, .. } => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("calls name functions")
                };
                self.call(expr, func, None, args)
            }
            Expr::MethodCall { receiver, args, .. } => {
                let def = self
                    .l
                    .typed
                    .method(expr)
                    .expect("checked method calls are resolved");
                let DefKind::Func(func) = self.l.names.def(def).kind else {
                    unreachable!("methods are functions")
                };
                self.call(expr, func, Some(*receiver), args)
            }
            _ => Rvalue::Use(self.operand(expr)),
        }
    }

    fn call(
        &mut self,
        expr: Id<Expr>,
        func: Id<Func>,
        receiver: Option<Id<Expr>>,
        args: &[Id<Expr>],
    ) -> Rvalue {
        let type_args = self.type_args(expr);
        let func = self.l.instance(func, type_args);
        let args = receiver
            .into_iter()
            .chain(args.iter().copied())
            .map(|a| self.operand(a))
            .collect();
        Rvalue::Call { func, args }
    }

    fn type_args(&mut self, expr: Id<Expr>) -> Vec<TypeId> {
        let args = self.l.typed.type_args(expr).to_vec();
        args.into_iter().map(|t| self.ty(t)).collect()
    }

    /// Where a parameter or local lives. An `out` parameter is a pointer to the caller's
    /// variable, and its name means what it points to.
    fn variable(&self, def: DefId) -> Option<Place> {
        let place = Place::local(*self.bindings.get(&def)?);
        Some(match self.l.names.def(def).kind {
            DefKind::Param { func, index } if self.l.ast.funcs[func].params[index].out => {
                place.project(Projection::Deref)
            }
            _ => place,
        })
    }

    /// Evaluates `expr` to a place, storing it in a temporary if it isn't one already.
    fn place(&mut self, expr: Id<Expr>) -> Place {
        let ast = self.l.ast;
        match &ast.exprs[expr] {
            Expr::Var(_) => {
                let def = self
                    .l
                    .names
                    .expr(expr)
                    .expect("checked programs are fully resolved");
                if let Some(place) = self.variable(def) {
                    return place;
                }
            }
            Expr::Field { object, .. } => {
                let index = self
                    .l
                    .typed
                    .field(expr)
                    .expect("checked fields are resolved");
                let object_ty = self.expr_ty(*object);
                let mut place = self.place(*object);
                // Fields are reachable through one reference.
                if matches!(self.l.mir.types.get(object_ty), Ty::Ref { .. }) {
                    place = place.project(Projection::Deref);
                }
                return place.project(Projection::Field(index));
            }
            Expr::Deref(inner) => return self.place(*inner).project(Projection::Deref),
            _ => {}
        }

        match self.operand(expr) {
            Operand::Copy(place) | Operand::Move(place) => place,
            constant => {
                let ty = self.expr_ty(expr);
                let span = self.span(expr);
                let temp = self.temp(ty, span.clone());
                self.assign(Place::local(temp), Rvalue::Use(constant), span);
                Place::local(temp)
            }
        }
    }
}

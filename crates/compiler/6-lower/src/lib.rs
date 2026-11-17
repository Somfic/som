//! Lowers the typed syntax tree to MIR, evaluating left to right. Generic functions are
//! monomorphized, and a value parameter's argument is part of the instance: `sum<3>` and
//! `sum<4>` are different functions, with `N` a constant in each.

mod builder;
mod builtins;
mod call;
mod drops;
mod elaborate;
mod expr;
mod failure;
mod instances;
mod interp;
mod loops;
mod matching;
mod optional;
mod place;
mod rvalue;
mod stmt;
mod traits;

use instances::Subst;
use som_ast::{Ast, Func, Id, Item, Span, Stmt};
use som_lang::LangTrait;
use som_mir::*;
use som_resolve::{DefId, DefKind, Names};
use som_typeck::{Typed, Value};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

pub fn lower(ast: &Ast, names: &Names, typed: &Typed) -> Mir {
    let mut lowerer = Lowerer::new(ast, names, typed);
    lowerer.run();
    lowerer.mir
}

/// Also checks each instance of a function checked per instance (`@if N >= 3`) as it's
/// reached; what's wrong in them, if anything, comes back instead of the MIR.
pub fn lower_checked(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
) -> Result<(Mir, Vec<CheckedInstance>), Vec<InstanceErrors>> {
    let mut lowerer = Lowerer::new(ast, names, typed);
    lowerer.run();
    if !lowerer.instance_errors.is_empty() {
        return Err(lowerer.instance_errors);
    }
    let mut checked = Vec::new();
    for ((func, args), typed) in std::mem::take(&mut lowerer.instance_typed) {
        let Some(&id) = lowerer.instances.get(&(func, args.clone())) else {
            continue;
        };
        let values = (names.type_params(func).iter().zip(&args))
            .filter_map(|(&def, &arg)| match lowerer.mir.types.get(arg) {
                Ty::Value(Value::Int(n)) => Some((def, *n as i128)),
                _ => None,
            })
            .collect();
        checked.push(CheckedInstance {
            name: lowerer.mir.funcs[id].name.clone(),
            used_at: (lowerer.requested_at.get(&id).cloned())
                .unwrap_or_else(|| ast.funcs.span(func).clone()),
            func,
            typed,
            values,
        });
    }
    Ok((lowerer.mir, checked))
}

/// For the prover to prove.
pub struct CheckedInstance {
    pub name: String,
    pub used_at: Span,
    pub func: Id<Func>,
    pub typed: Rc<Typed>,
    pub values: Vec<(DefId, i128)>,
}

/// What's wrong in one instance's `@if`s over its generic parameters.
pub struct InstanceErrors {
    pub name: String,
    pub used_at: Span,
    pub errors: Vec<som_typeck::TypeError>,
}

/// Only what working out `consts` runs: their initializers and what those call. Returns
/// each one's initializer, in order.
pub fn lower_inits(
    ast: &Ast,
    names: &Names,
    typed: &Typed,
    consts: &[Id<som_ast::Const>],
) -> (Mir, Vec<FuncId>) {
    let mut lowerer = Lowerer::new(ast, names, typed);
    lowerer.prepare();
    let inits = consts.iter().map(|&c| lowerer.init(c, None)).collect();
    lowerer.finish();
    (lowerer.mir, inits)
}

/// For the borrow checker only, nothing is generated from it: each generic function once,
/// with stand-ins for its parameters, so a use after a move is found where it's written,
/// not only once someone uses it with a type that moves.
pub fn lower_generic(ast: &Ast, names: &Names, typed: &Typed) -> Mir {
    let mut lowerer = Lowerer::new(ast, names, typed);
    lowerer.run_generic();
    lowerer.mir
}

struct Lowerer<'a> {
    ast: &'a Ast,
    names: &'a Names,
    typed: &'a Typed,
    mir: Mir,
    instances: HashMap<(Id<Func>, Vec<TypeId>), FuncId>,
    queue: Vec<(FuncId, Job)>,
    /// By constant and the function instance it's in.
    inits: HashMap<(Id<som_ast::Const>, Owner), FuncId>,
    /// Constants of modules and impl blocks: none of them is in a function.
    item_consts: HashSet<Id<som_ast::Const>>,
    /// What checking each per-instance function's instance found.
    instance_typed: HashMap<(Id<Func>, Vec<TypeId>), Rc<Typed>>,
    instance_errors: Vec<InstanceErrors>,
    /// Where each instance was first asked for: `at` when it was.
    requested_at: HashMap<FuncId, Span>,
    at: Option<Span>,
    /// `Some(None)` if there's no standard library to say what a failure is.
    failed: Option<Option<FuncId>>,
    /// Method → its impl's type, as a name C can have.
    owners: HashMap<Id<Func>, String>,
    /// A trait's method → the trait, and how many generic parameters it has.
    trait_methods: HashMap<Id<Func>, (DefId, usize)>,
    /// `None` for a type with nothing to drop.
    drop_glues: HashMap<TypeId, Option<FuncId>>,
}

impl<'a> Lowerer<'a> {
    fn new(ast: &'a Ast, names: &'a Names, typed: &'a Typed) -> Self {
        Lowerer {
            ast,
            names,
            typed,
            mir: Mir::new(typed.table.clone()),
            instances: HashMap::new(),
            queue: Vec::new(),
            inits: HashMap::new(),
            item_consts: HashSet::new(),
            instance_typed: HashMap::new(),
            instance_errors: Vec::new(),
            requested_at: HashMap::new(),
            at: None,
            failed: None,
            owners: HashMap::new(),
            trait_methods: HashMap::new(),
            drop_glues: HashMap::new(),
        }
    }

    fn run(&mut self) {
        self.prepare();
        // Every non-generic function, whether or not anything calls it. Generic ones are
        // added as they're called.
        for (id, func) in self.ast.funcs.iter() {
            if func.generics.is_empty() && !self.is_alias(id) {
                self.instance(id, Vec::new());
            }
        }
        // `@assert(…);`: run while compiling for what it checks.
        for (c, constant) in self.ast.consts.iter() {
            if !constant.is_named() && !constant.in_place {
                self.init(c, None);
            }
        }
        self.finish();
    }

    fn run_generic(&mut self) {
        self.prepare();
        for (id, func) in self.ast.funcs.iter() {
            let params = self.names.type_params(id).to_vec();
            // One checked per instance has nothing to check generically in its `@if`s.
            if params.is_empty()
                || func.body.is_none()
                || self.is_alias(id)
                || self.typed.is_per_instance(id)
            {
                continue;
            }
            let args = params.iter().map(|&param| self.stand_in(param)).collect();
            self.instance(id, args);
        }
        self.finish();
    }

    /// A struct of its own with nothing in it, copied only if `param` has to be `Copy`; for a
    /// value parameter, `1`.
    fn stand_in(&mut self, param: DefId) -> TypeId {
        if self.names.is_value_param(param) {
            return self.mir.types.intern(Ty::Value(Value::Int(1)));
        }
        let ty = self.mir.types.intern(Ty::Struct(param, Vec::new()));
        if !self.mir.has_struct(param, &[]) {
            let copy = (self.names.lang(LangTrait::Copy))
                .is_some_and(|copy| self.names.bounds(param).contains(&copy));
            self.mir.add_struct(StructDef {
                def: param,
                args: Vec::new(),
                name: self.names.def(param).name.to_string(),
                fields: Vec::new(),
                variants: None,
                copy,
                handle: false,
            });
        }
        ty
    }

    /// A function only an alias names isn't lowered on its own.
    fn is_alias(&self, id: Id<Func>) -> bool {
        !matches!(self.names.def(self.names.func(id)).kind, DefKind::Func(f) if f == id)
    }

    fn prepare(&mut self) {
        let ast = self.ast;
        for (id, t) in ast.traits.iter() {
            let def = self.names.trait_def(id);
            for &method in &t.methods {
                self.trait_methods.insert(method, (def, t.generics.len()));
            }
        }
        for module in &ast.modules {
            for item in &module.items {
                if let Item::Impl(block) = item {
                    for &method in &block.methods {
                        // A name C can have: `bool`, or `_i32__3_` for `[i32; 3]`.
                        let owner = (block.target.name().name.chars())
                            .map(|c| if c.is_alphanumeric() { c } else { '_' })
                            .collect();
                        self.owners.insert(method, owner);
                    }
                }
            }
        }

        for module in &ast.modules {
            for item in &module.items {
                match item {
                    Item::Const(c) => {
                        self.item_consts.insert(*c);
                    }
                    Item::Impl(block) => self.item_consts.extend(&block.consts),
                    _ => {}
                }
            }
        }

        // Generic structs get a copy for each set of type arguments used, once functions are
        // lowered.
        for (id, s) in ast.structs.iter() {
            if s.generics.is_empty() {
                self.struct_instance(id, Vec::new());
            }
        }
    }

    fn finish(&mut self) {
        while let Some((func_id, job)) = self.queue.pop() {
            let body = match job {
                Job::Func(func, type_args)
                    if self.typed.is_per_instance(func)
                        && !type_args.is_empty()
                        && !self.check_instance(func_id, func, &type_args) =>
                {
                    unreachable_body(&self.mir.funcs[func_id], self.ast.funcs.span(func))
                }
                Job::Func(func, type_args) => FnBuilder::new(self, func, type_args).build(),
                Job::Init(c, owner) => FnBuilder::for_init(self, c, owner).build_init(c),
            };
            self.mir.funcs[func_id].body = Some(body);
        }

        // A copy's fields can use more generic structs, so until there are no new ones.
        loop {
            let types = &self.mir.types;
            let wanted: Vec<(DefId, Vec<TypeId>)> = types
                .iter()
                .filter_map(|(id, ty)| match ty {
                    Ty::Struct(def, args)
                        if !args.is_empty()
                            && concrete(types, id)
                            && !self.mir.has_struct(*def, args) =>
                    {
                        Some((*def, args.clone()))
                    }
                    _ => None,
                })
                .collect();
            if wanted.is_empty() {
                break;
            }
            for (def, args) in wanted {
                let DefKind::Struct(s) = self.names.def(def).kind else {
                    unreachable!("struct types name structs")
                };
                self.struct_instance(s, args);
            }
        }
    }

    /// Like `Mir::is_copy`, but before the copies of generic structs exist: copying is the
    /// struct's, whatever its type arguments.
    fn is_copy(&self, ty: TypeId) -> bool {
        match self.mir.types.get(ty) {
            Ty::Struct(def, args) => match self.names.def(*def).kind {
                DefKind::Struct(s) => self.ast.structs[s].copy || self.implements_copy(*def),
                // A type parameter's stand-in, in `lower_generic`.
                _ if self.mir.has_struct(*def, args) => self.mir.struct_def(*def, args).copy,
                _ => true,
            },
            Ty::Ref { mutable: true, .. } => false,
            Ty::Optional(inner) | Ty::Array { element: inner, .. } => self.is_copy(*inner),
            _ => true,
        }
    }

    fn share_impl(&mut self, ty: TypeId) -> Option<FuncId> {
        let trait_ = self.names.lang(LangTrait::Share)?;
        let (index, args) = som_typeck::traits::find_impl(
            &self.typed.impls,
            &self.mir.types,
            trait_,
            ty,
            &[],
            &|t| t,
        )?;
        let method = (self.typed.impls[index].methods.iter().copied())
            .find(|&m| Some(&*self.ast.funcs[m].name.name) == LangTrait::Share.method())?;
        Some(self.instance(method, args))
    }

    fn implements_copy(&self, def: DefId) -> bool {
        let Some(copy) = self.names.lang(LangTrait::Copy) else {
            return false;
        };
        (self.typed.impls.iter()).any(|info| {
            info.trait_ == Some(copy)
                && info
                    .target
                    .is_some_and(|t| matches!(self.mir.types.get(t), Ty::Struct(d, _) if *d == def))
        })
    }

    /// Naming a type, `List<List<i32>>`, needs them.
    fn make_structs_in(&mut self, ty: TypeId) {
        let t = self.mir.types.get(ty).clone();
        for child in t.children() {
            self.make_structs_in(child);
        }
        if let Ty::Struct(def, args) = t
            && !args.is_empty()
            && concrete(&self.mir.types, ty)
            && !self.mir.has_struct(def, &args)
            && let DefKind::Struct(s) = self.names.def(def).kind
        {
            self.struct_instance(s, args);
        }
    }
}

/// The program's, or one instance's.
#[derive(Clone)]
enum TypedRef<'a> {
    Global(&'a Typed),
    Instance(Rc<Typed>),
}

impl std::ops::Deref for TypedRef<'_> {
    type Target = Typed;

    fn deref(&self) -> &Typed {
        match self {
            TypedRef::Global(typed) => typed,
            TypedRef::Instance(typed) => typed,
        }
    }
}

/// For an instance with errors, said instead of compiled.
fn unreachable_body(func: &Function, span: &Span) -> Body {
    let mut locals = Arena::new();
    for &ty in std::iter::once(&func.ret).chain(&func.params) {
        locals.alloc(LocalDecl {
            ty,
            name: None,
            mutable: false,
            span: span.clone(),
        });
    }
    let mut blocks = Arena::new();
    blocks.alloc(Block {
        stmts: Vec::new(),
        terminator: Terminator {
            kind: TerminatorKind::Unreachable,
            span: span.clone(),
        },
    });
    Body {
        locals,
        arg_count: func.params.len(),
        outs: Vec::new(),
        two_phase: Vec::new(),
        blocks,
    }
}

enum Job {
    Func(Id<Func>, Vec<TypeId>),
    /// A `@const`'s initializer, in the function instance it's in.
    Init(Id<som_ast::Const>, Owner),
}

/// `None` outside any generic function instance, or in one with no parameters.
type Owner = Option<(Id<Func>, Vec<TypeId>)>;

#[derive(Clone, Copy)]
struct LoopTargets {
    exit: BlockId,
    /// The start of a `loop`, a `while`'s condition, a `for`'s step to the next value.
    next: BlockId,
    /// How many scopes were open where it started: those opened since end at either.
    depth: usize,
    /// The `for` loop, which `break x;` and `continue x;` name by its variable.
    stmt: Option<Id<Stmt>>,
}

fn concrete(types: &TypeTable, ty: TypeId) -> bool {
    match types.get(ty) {
        Ty::Param(_) | Ty::Var(_) | Ty::Error => false,
        t => t.children().into_iter().all(|child| concrete(types, child)),
    }
}

struct FnBuilder<'l, 'a> {
    l: &'l mut Lowerer<'a>,
    /// `None` for a `@const`'s initializer.
    func: Option<Id<Func>>,
    /// Whose generic parameters it has: its own, or for an initializer, its constant's.
    owner: Owner,
    typed: TypedRef<'a>,
    span: Span,
    subst: Subst<'a>,
    locals: Arena<LocalDecl>,
    /// Each starts out `Unreachable`, and stays that way if nothing terminates it.
    blocks: Arena<Block>,
    terminated: HashSet<BlockId>,
    current: BlockId,
    bindings: HashMap<DefId, Local>,
    /// The `let` locals of each enclosing scope.
    scopes: Vec<Vec<Local>>,
    loops: Vec<LoopTargets>,
    /// Places the source read, with where, not yet claimed by the statement using them.
    reads: Vec<(Place, Span)>,
    two_phase: Vec<Local>,
}

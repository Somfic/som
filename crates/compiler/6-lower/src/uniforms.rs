//! `som::gpu::each_uniform`, written for each type of uniforms: a struct's fields (going into
//! the program's own structs among them, `camera.pos`) or a shader closure's captures, each
//! one's index, name and value as a `som::gpu::Uniform` passed to the host's `set`. The
//! names are the GPU's: the shader's uniforms, as `som_shader` declares them.

use som_lang::{LangStruct, LangTrait};
use som_mir::*;
use som_resolve::DefKind;

use super::Lowerer;

/// A uniform: its name, where it is from the `&U` (after its `Deref`), and its type.
struct Leaf {
    name: String,
    path: Vec<Projection>,
    ty: TypeId,
}

impl Lowerer<'_> {
    /// The body of `each_uniform<U, F>(uniforms: &U, set: F)`, for `U` `uniforms` and `F`
    /// `set`: a call of `set` for each uniform the host can be given.
    pub(super) fn each_uniform_body(&mut self, uniforms: TypeId, set: TypeId, span: som_base::Span) -> Body {
        let mut leaves = Vec::new();
        self.uniform_leaves(uniforms, "", Vec::new(), &mut leaves, 0);

        let usize = self.mir.types.usize;
        let cstr = self.mir.types.cstr;
        let unit = self.mir.types.unit;
        let lent_u = self.mir.types.intern(Ty::Ref {
            mutable: false,
            to: uniforms,
        });
        let uniform = (self.names.lang_struct(LangStruct::Uniform))
            .map(|def| self.mir.types.intern(Ty::Struct(def, Vec::new())));
        let from = self.from_method();

        let mut locals = Arena::new();
        let mut local = |ty, name: Option<&str>| {
            locals.alloc(LocalDecl {
                ty,
                name: name.map(str::to_string),
                mutable: false,
                span: span.clone(),
            })
        };
        local(unit, None);
        let param_u = local(lent_u, Some("uniforms"));
        let param_set = local(set, Some("set"));
        let stmt = |kind| Statement {
            kind,
            span: span.clone(),
            reads: Vec::new(),
        };
        let mut stmts = Vec::new();

        if let (Some(uniform), Some((from_trait, from))) = (uniform, from) {
            self.make_structs_in(uniform);
            let body = self.closure_instance(set);
            let item = self.fn_item_instance(set);
            for (i, leaf) in leaves.into_iter().enumerate() {
                let lent_ty = self.mir.types.intern(Ty::Ref {
                    mutable: false,
                    to: leaf.ty,
                });
                // A field the host can't be given is the shader's to report.
                let fits = som_typeck::traits::find_impl(
                    &self.typed.impls,
                    &self.mir.types,
                    from_trait,
                    uniform,
                    &[lent_ty],
                    &|t| t,
                )
                .is_some();
                if !fits {
                    continue;
                }
                let convert = self.instance(from, vec![uniform, lent_ty]);

                let lent = local(lent_ty, None);
                let value = local(uniform, None);
                let done = local(unit, None);
                let mut place = Place::local(param_u).project(Projection::Deref);
                for p in leaf.path {
                    place = place.project(p);
                }
                stmts.push(stmt(StatementKind::StorageLive(lent)));
                stmts.push(stmt(StatementKind::Assign(
                    Place::local(lent),
                    Rvalue::Ref {
                        mutable: false,
                        place,
                    },
                )));
                stmts.push(stmt(StatementKind::StorageLive(value)));
                stmts.push(stmt(StatementKind::Assign(
                    Place::local(value),
                    Rvalue::Call {
                        func: convert,
                        args: vec![Operand::Move(Place::local(lent))],
                    },
                )));
                let mut args = vec![
                    Operand::Const(Const {
                        kind: ConstKind::Int(i as i128),
                        ty: usize,
                    }),
                    Operand::Const(Const {
                        kind: ConstKind::Str(leaf.name),
                        ty: cstr,
                    }),
                    Operand::Move(Place::local(value)),
                ];
                let call = match (body, item) {
                    // A closure's body, given the closure lent.
                    (Some(func), _) => {
                        let lent_set_ty = self.mir.types.intern(Ty::Ref {
                            mutable: false,
                            to: set,
                        });
                        let lent_set = local(lent_set_ty, None);
                        stmts.push(stmt(StatementKind::StorageLive(lent_set)));
                        stmts.push(stmt(StatementKind::Assign(
                            Place::local(lent_set),
                            Rvalue::Ref {
                                mutable: false,
                                place: Place::local(param_set),
                            },
                        )));
                        args.insert(0, Operand::Move(Place::local(lent_set)));
                        Rvalue::Call { func, args }
                    }
                    (None, Some(func)) => Rvalue::Call { func, args },
                    (None, None) => Rvalue::CallIndirect {
                        callee: Operand::Copy(Place::local(param_set)),
                        args,
                    },
                };
                stmts.push(stmt(StatementKind::StorageLive(done)));
                stmts.push(stmt(StatementKind::Assign(Place::local(done), call)));
            }
        }
        let mut blocks = Arena::new();
        blocks.alloc(Block {
            stmts,
            terminator: Terminator {
                kind: TerminatorKind::Return,
                span: span.clone(),
            },
        });
        Body {
            locals,
            arg_count: 2,
            outs: Vec::new(),
            two_phase: Vec::new(),
            blocks,
        }
    }

    /// `From`, and its `from`.
    fn from_method(&self) -> Option<(som_resolve::DefId, som_ast::Id<som_ast::Func>)> {
        let from = self.names.lang(LangTrait::From)?;
        let method = (self.trait_methods.iter())
            .find(|(f, (t, _))| *t == from && &*self.ast.funcs[**f].name.name == "from")
            .map(|(f, _)| *f)?;
        Some((from, method))
    }

    /// `ty`'s uniforms, from `path`, named after `prefix`: at the top, a struct's fields or a
    /// closure's captures; below it, the fields of the program's own structs; anything else
    /// is one.
    fn uniform_leaves(&mut self, ty: TypeId, prefix: &str, path: Vec<Projection>, out: &mut Vec<Leaf>, depth: usize) {
        let into = match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => match self.names.def(def).kind {
                DefKind::Closure(_) => depth == 0,
                DefKind::Struct(_) => {
                    let s = self.mir.struct_def(def, &args);
                    let plain = s.variants.is_none() && !s.handle;
                    let module = &self.ast.modules[self.names.def(def).module];
                    let own = args.is_empty()
                        && !self.mir.lang_structs.contains_key(&def)
                        && !som_lang::is_std_module(&module.name)
                        && !module.path.to_string_lossy().starts_with('<');
                    plain && (depth == 0 || own) && depth < 16
                }
                _ => false,
            },
            _ => false,
        };
        if !into {
            out.push(Leaf {
                name: prefix.trim_end_matches('.').to_string(),
                path,
                ty,
            });
            return;
        }
        let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
            unreachable!("gone into structs only")
        };
        let fields = self.mir.struct_def(def, &args).fields.clone();
        for (i, (name, field)) in fields.into_iter().enumerate() {
            if matches!(self.mir.types.get(field), Ty::Unit) {
                continue;
            }
            let mut at = path.clone();
            at.push(Projection::Field(i));
            // A capture that doesn't copy is lent: what it refers to.
            let field = match *self.mir.types.get(field) {
                Ty::Ref { to, .. } if depth == 0 => {
                    at.push(Projection::Deref);
                    to
                }
                _ => field,
            };
            // Below the top, a struct's member, as GLSL names it.
            let name = match depth {
                0 => name,
                _ => som_lang::glsl_member(&name),
            };
            self.uniform_leaves(field, &format!("{prefix}{name}."), at, out, depth + 1);
        }
    }
}

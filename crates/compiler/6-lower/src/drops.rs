//! A type's drop glue is a function taking a reference to a value: it calls the type's
//! `Drop::drop`, if it implements it, then drops each field that needs it.

use std::collections::HashMap;

use som_ast::Id;
use som_base::Span;
use som_lang::{LangStruct, LangTrait};
use som_mir::*;
use som_typeck::traits::find_impl;

use super::Lowerer;
use super::instances::internal_function;

/// What a type's glue drops.
enum Shape {
    Struct {
        own: Option<FuncId>,
        /// Fields every value has: an enum's common fields.
        common: Vec<(usize, FuncId)>,
        /// Each variant's own fields.
        variants: Vec<Vec<(usize, FuncId)>>,
    },
    Array {
        len: u64,
        element: FuncId,
    },
    Optional(FuncId),
}

impl Lowerer<'_> {
    /// `None` if nothing needs doing.
    pub(super) fn drop_glue(&mut self, ty: TypeId) -> Option<FuncId> {
        if let Some(&glue) = self.drop_glues.get(&ty) {
            return glue;
        }
        // Until it's built: a type can't hold itself, other than behind a pointer.
        self.drop_glues.insert(ty, None);
        let glue = self.build_glue(ty);
        self.drop_glues.insert(ty, glue);
        glue
    }

    /// `ty`'s own `Drop::drop`, for its type arguments.
    fn drop_impl(&mut self, ty: TypeId) -> Option<FuncId> {
        let trait_ = self.names.lang(LangTrait::Drop)?;
        let (index, args) = find_impl(&self.typed.impls, &self.mir.types, trait_, ty, &[], &|t| t)?;
        let method = (self.typed.impls[index].methods.iter().copied())
            .find(|&m| Some(&*self.ast.funcs[m].name.name) == LangTrait::Drop.method())?;
        self.mir.drop_types.insert(ty);
        Some(self.instance(method, args))
    }

    fn build_glue(&mut self, ty: TypeId) -> Option<FuncId> {
        let shape = self.glue_shape(ty)?;
        let span = self.span_of_type(ty);
        let unit = self.mir.types.unit;
        let reference = self.mir.types.intern(Ty::Ref {
            mutable: true,
            to: ty,
        });
        let mut g = Glue::new(span.clone(), unit, reference);
        let this = Place::local(Id::new(1)).project(Projection::Deref);
        match shape {
            Shape::Struct {
                own,
                common,
                variants,
            } => {
                if let Some(own) = own {
                    g.call_on(own, this.clone(), reference);
                }
                self.drop_fields(&mut g, ty, &this, common);
                self.drop_variants(&mut g, ty, &this, variants);
            }
            Shape::Array { len, element } => self.array_glue(&mut g, ty, this, len, element),
            Shape::Optional(inner) => self.optional_glue(&mut g, ty, this, inner),
        }
        g.assign(
            Place::local(RETURN),
            Rvalue::Use(Operand::Const(Const {
                kind: ConstKind::Unit,
                ty: unit,
            })),
        );
        g.terminate(TerminatorKind::Return);

        let name = format!("drop<{}>", self.mir.display_ty(ty));
        let mut function = internal_function(name, vec![reference], unit, span);
        function.body = Some(Body {
            locals: g.locals,
            arg_count: 1,
            outs: Vec::new(),
            two_phase: Vec::new(),
            blocks: g.blocks,
        });
        Some(self.mir.funcs.alloc(function))
    }

    /// `None` if nothing needs doing.
    fn glue_shape(&mut self, ty: TypeId) -> Option<Shape> {
        match self.mir.types.get(ty).clone() {
            // What a `Manual` holds is dropped by whatever holds it, not on its own.
            Ty::Struct(def, _) if self.names.lang_struct(LangStruct::Manual) == Some(def) => None,
            Ty::Struct(def, args) => self.struct_shape(ty, def, &args),
            Ty::Array { element, len } => {
                let element = self.drop_glue(element)?;
                Some(Shape::Array {
                    len: self.mir.types.len(len),
                    element,
                })
            }
            Ty::Optional(inner) => Some(Shape::Optional(self.drop_glue(inner)?)),
            _ => None,
        }
    }

    fn struct_shape(&mut self, ty: TypeId, def: DefId, args: &[TypeId]) -> Option<Shape> {
        self.make_structs_in(ty);
        let own = self.drop_impl(ty);
        let strukt = self.mir.struct_def(def, args);
        let fields: Vec<TypeId> = strukt.fields.iter().map(|(_, t)| *t).collect();
        let variants: Option<Vec<Vec<usize>>> =
            (strukt.variants.as_ref()).map(|vs| vs.iter().map(|v| v.fields.clone()).collect());
        let mut glues = HashMap::new();
        for (i, &field) in fields.iter().enumerate() {
            if let Some(glue) = self.drop_glue(field) {
                glues.insert(i, glue);
            }
        }
        let with_glue = |indexes: &mut dyn Iterator<Item = usize>| -> Vec<(usize, FuncId)> {
            indexes
                .filter_map(|i| glues.get(&i).map(|&g| (i, g)))
                .collect()
        };
        let (common, variants) = match variants {
            // The tag, then fields in every variant, then each variant's own.
            Some(variants) => {
                let common: Vec<usize> = (1..fields.len())
                    .filter(|i| variants.iter().all(|v| v.contains(i)))
                    .collect();
                let own: Vec<Vec<(usize, FuncId)>> = (variants.iter())
                    .map(|v| with_glue(&mut v.iter().copied().filter(|i| !common.contains(i))))
                    .collect();
                (with_glue(&mut common.into_iter()), own)
            }
            None => (with_glue(&mut (0..fields.len())), Vec::new()),
        };
        if own.is_none() && common.is_empty() && variants.iter().all(Vec::is_empty) {
            return None;
        }
        Some(Shape::Struct {
            own,
            common,
            variants,
        })
    }

    fn drop_fields(
        &mut self,
        g: &mut Glue,
        ty: TypeId,
        this: &Place,
        fields: Vec<(usize, FuncId)>,
    ) {
        for (i, glue) in fields {
            let field = self.field_ty(ty, i);
            let place = this.clone().project(Projection::Field(i));
            g.drop_field(glue, place, field, &mut self.mir.types);
        }
    }

    /// Only the variant the tag says holds anything.
    fn drop_variants(
        &mut self,
        g: &mut Glue,
        ty: TypeId,
        this: &Place,
        variants: Vec<Vec<(usize, FuncId)>>,
    ) {
        let u32 = self.mir.types.intern(Ty::Int(IntTy::U32));
        let bool = self.mir.types.bool;
        for (v, fields) in variants.into_iter().enumerate() {
            if fields.is_empty() {
                continue;
            }
            let is = g.local(bool);
            let tag = Operand::Copy(this.clone().project(Projection::Field(0)));
            let value = Operand::Const(Const {
                kind: ConstKind::Int(v as i128),
                ty: u32,
            });
            g.assign(
                Place::local(is),
                Rvalue::Binary {
                    op: BinOp::Equals,
                    lhs: tag,
                    rhs: value,
                },
            );
            let (then, after) = (g.block(), g.block());
            g.branch(Operand::Copy(Place::local(is)), then, after);
            g.current = then;
            self.drop_fields(g, ty, this, fields);
            g.goto(after);
            g.current = after;
        }
    }

    /// Each element, by index.
    fn array_glue(&mut self, g: &mut Glue, ty: TypeId, this: Place, len: u64, element: FuncId) {
        let usize = self.mir.types.usize;
        let bool = self.mir.types.bool;
        let Ty::Array {
            element: element_ty,
            ..
        } = *self.mir.types.get(ty)
        else {
            unreachable!("an array's glue")
        };
        let i = g.local(usize);
        let int = |n: i128| {
            Operand::Const(Const {
                kind: ConstKind::Int(n),
                ty: usize,
            })
        };
        g.assign(Place::local(i), Rvalue::Use(int(0)));
        let (header, body, exit) = (g.block(), g.block(), g.block());
        g.goto(header);
        g.current = header;
        let more = g.local(bool);
        g.assign(
            Place::local(more),
            Rvalue::Binary {
                op: BinOp::LessThan,
                lhs: Operand::Copy(Place::local(i)),
                rhs: int(len as i128),
            },
        );
        g.branch(Operand::Copy(Place::local(more)), body, exit);
        g.current = body;
        let item = this.project(Projection::Index(i));
        g.drop_field(element, item, element_ty, &mut self.mir.types);
        g.assign(
            Place::local(i),
            Rvalue::Binary {
                op: BinOp::Add,
                lhs: Operand::Copy(Place::local(i)),
                rhs: int(1),
            },
        );
        g.goto(header);
        g.current = exit;
    }

    /// What it holds, if it holds something.
    fn optional_glue(&mut self, g: &mut Glue, ty: TypeId, this: Place, inner: FuncId) {
        let Ty::Optional(inner_ty) = *self.mir.types.get(ty) else {
            unreachable!("an optional's glue")
        };
        let (then, after) = (g.block(), g.block());
        let holds = Operand::Copy(this.clone().project(Projection::Field(0)));
        g.branch(holds, then, after);
        g.current = then;
        let value = this.project(Projection::Field(1));
        g.drop_field(inner, value, inner_ty, &mut self.mir.types);
        g.goto(after);
        g.current = after;
    }

    fn field_ty(&self, ty: TypeId, index: usize) -> TypeId {
        self.mir
            .project(ty, Projection::Field(index))
            .expect("a field of the struct")
    }

    /// The struct's, or for other types the program's start.
    fn span_of_type(&self, ty: TypeId) -> Span {
        match *self.mir.types.get(ty) {
            Ty::Struct(def, _) => self.names.def(def).span.clone(),
            _ => (self.ast.funcs.iter().next())
                .map(|(id, _)| self.ast.funcs.span(id).clone())
                .expect("a program has a function"),
        }
    }
}

/// A drop glue function being built.
struct Glue {
    locals: Arena<LocalDecl>,
    blocks: Arena<Block>,
    current: BlockId,
    span: Span,
    unit: TypeId,
}

impl Glue {
    fn new(span: Span, unit: TypeId, reference: TypeId) -> Glue {
        let mut g = Glue {
            locals: Arena::new(),
            blocks: Arena::new(),
            current: Id::new(0),
            span,
            unit,
        };
        g.local(unit);
        g.local(reference);
        g.current = g.block();
        g
    }

    fn local(&mut self, ty: TypeId) -> Local {
        self.locals.alloc(LocalDecl {
            ty,
            name: None,
            mutable: true,
            span: self.span.clone(),
        })
    }

    fn block(&mut self) -> BlockId {
        self.blocks.alloc(Block {
            stmts: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Unreachable,
                span: self.span.clone(),
            },
        })
    }

    fn assign(&mut self, place: Place, rvalue: Rvalue) {
        let span = self.span.clone();
        self.blocks[self.current].stmts.push(Statement {
            kind: StatementKind::Assign(place, rvalue),
            span,
            reads: Vec::new(),
        });
    }

    fn terminate(&mut self, kind: TerminatorKind) {
        self.blocks[self.current].terminator = Terminator {
            kind,
            span: self.span.clone(),
        };
    }

    fn goto(&mut self, target: BlockId) {
        self.terminate(TerminatorKind::Goto(target));
    }

    fn branch(&mut self, condition: Operand, then: BlockId, otherwise: BlockId) {
        self.terminate(TerminatorKind::Branch {
            condition,
            then,
            otherwise,
        });
    }

    /// `func(&mut place)`, with `&mut place` a `reference`.
    fn call_on(&mut self, func: FuncId, place: Place, reference: TypeId) {
        let lent = self.local(reference);
        self.assign(
            Place::local(lent),
            Rvalue::Ref {
                mutable: true,
                place,
            },
        );
        let done = self.local(self.unit);
        self.assign(
            Place::local(done),
            Rvalue::Call {
                func,
                args: vec![Operand::Move(Place::local(lent))],
            },
        );
    }

    fn drop_field(&mut self, glue: FuncId, place: Place, ty: TypeId, types: &mut TypeTable) {
        let reference = types.intern(Ty::Ref {
            mutable: true,
            to: ty,
        });
        self.call_on(glue, place, reference);
    }
}

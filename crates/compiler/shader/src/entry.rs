//! What's around a shader: its inputs, uniforms and output, and the entry point that calls
//! it.

use som_base::message::{code, field};
use som_base::{Message, msg};
use som_lang::LangStruct;
use som_mir::*;

use crate::Translator;
use crate::error::{NoGpu, Problem};
use crate::names::ident;
use crate::types::Scalar;

/// A vertex shader drawing one triangle over the whole of what's drawn to (the
/// viewport, which the host sets to where the shader goes): `uv` from (0, 0) at its top left
/// to (1, 1) at its bottom right.
const VERTEX: &str = "struct SomVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> SomVertex {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: SomVertex;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    out.color = vec4<f32>(1.0);
    return out;
}
";

/// The uniforms, as they're declared.
#[derive(Default)]
struct Wgsl {
    /// `SomUniforms`' members.
    members: Vec<String>,
    member_names: std::collections::HashSet<String>,
    /// Each texture and storage buffer, declared, in order.
    bound: Vec<String>,
}

impl Wgsl {
    /// A member for the uniform at `path`, `camera.pos`: `camera_pos`.
    fn member(&mut self, path: &str) -> String {
        let base = crate::names::member(&ident(path));
        let mut name = base.clone();
        let mut n = 2;
        while !self.member_names.insert(name.clone()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        name
    }
}

#[derive(Default)]
pub(crate) struct Main {
    /// Inputs, uniforms and output, before everything else.
    pub globals: String,
    pub main: String,
}

/// What a shader's parameter is given.
enum Param {
    /// A `Fragment` or `&Fragment`.
    Fragment(TypeId),
    /// A `&` to a struct of uniforms.
    Uniforms(TypeId),
    Unit,
}

impl Translator<'_> {
    pub(crate) fn fragment(&mut self) -> Main {
        match self.try_fragment() {
            Ok(main) => main,
            Err(problem) => {
                self.fail(self.entry, problem);
                Main::default()
            }
        }
    }

    fn try_fragment(&mut self) -> Result<Main, Problem> {
        let func = &self.mir.funcs[self.entry];
        let span = crate::head(&func.span);
        let body = func.body.as_ref().expect("a shader has a body");
        let problem = |message: Message, label: Message, hint: Option<Message>| Problem {
            message,
            span: span.clone(),
            label,
            hint,
        };
        let shape = || {
            Some(msg![
                "a fragment shader takes a ",
                code("Fragment"),
                ", a ",
                code("&"),
                " to a struct of uniforms, or both, and gives a ",
                code("Vector<4, f32>"),
                ": the pixel's colour"
            ])
        };

        if self.vector(func.ret) != Some((Scalar::Float, 4)) {
            return Err(problem(
                msg!["a fragment shader gives the pixel's colour, not a ", self.ty_part(func.ret)],
                msg!["it returns a ", self.ty_part(func.ret)],
                shape(),
            ));
        }
        let mut params = Vec::new();
        let mut uniforms = None;
        for (i, &param) in func.params.iter().enumerate() {
            let param_span = body.locals[Local::new(i + 1)].span.clone();
            let (by_ref, to) = match self.mir.types.get(param) {
                Ty::Ref { mutable: false, to } => (true, *to),
                _ => (false, param),
            };
            params.push(if self.is_unit(param) {
                Param::Unit
            } else if self.is_lang(to, LangStruct::Fragment) {
                Param::Fragment(to)
            } else if by_ref
                && matches!(self.mir.types.get(to), Ty::Struct(def, args)
                    if self.mir.struct_def(*def, args).variants.is_none())
                && uniforms.replace(to).is_none()
            {
                Param::Uniforms(to)
            } else {
                return Err(Problem {
                    span: param_span,
                    ..problem(
                        msg!["a fragment shader can't take a ", self.ty_part(param)],
                        msg!["this is a ", self.ty_part(param)],
                        shape(),
                    )
                });
            });
        }

        self.uniforms = uniforms;
        let (globals, uniform_args) = self.uniform_fields(uniforms, &span)?;

        let mut args = Vec::new();
        for param in params {
            args.push(match param {
                Param::Unit => continue,
                Param::Fragment(ty) => {
                    let members = (self.members(ty)).map_err(|no| no.at(&span, "it".into()))?;
                    let Ty::Struct(def, struct_args) = self.mir.types.get(ty).clone() else {
                        unreachable!("`Fragment` is a struct")
                    };
                    let fields = self.mir.struct_def(def, &struct_args).fields.clone();
                    let mut parts = Vec::new();
                    for (_, _, index) in members {
                        parts.push(match fields[index].0.as_str() {
                            "uv" => "v.uv",
                            "color" => "v.color",
                            "position" => "v.position",
                            other => unreachable!("`Fragment` has no `{other}`"),
                        });
                    }
                    let name = self.ty(ty).map_err(|no| no.at(&span, "it".into()))?;
                    format!("{name}({})", parts.join(", "))
                }
                Param::Uniforms(ty) => {
                    let name = self.ty(ty).map_err(|no| no.at(&span, "its uniforms".into()))?;
                    match uniform_args.is_empty() {
                        true => format!("{name}(false)"),
                        false => format!("{name}({})", uniform_args.join(", ")),
                    }
                }
            });
        }

        let shader = self.func_name(self.entry);
        let main = format!(
            "{VERTEX}\n@fragment\nfn fs_main(v: SomVertex) -> @location(0) vec4<f32> {{\n    return {shader}({});\n}}\n",
            args.join(", "),
        );
        Ok(Main { globals, main })
    }

    /// The globals declaring uniforms `ty`, and each of its members made from them, checking
    /// each can be one.
    pub(crate) fn uniform_fields(&mut self, uniforms: Option<TypeId>, span: &som_base::Span) -> Result<(String, Vec<String>), Problem> {
        let problem = |message: Message, label: Message, hint: Option<Message>| Problem {
            message,
            span: span.clone(),
            label,
            hint,
        };
        if let Some(ty) = uniforms {
            let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
                unreachable!("uniforms are a struct")
            };
            let fields = self.mir.struct_def(def, &args).fields.clone();
            // A closure's captures are its uniforms.
            let closure = self.mir.struct_def(def, &args).name == "closure";
            let struct_name = self.display(ty);
            self.uniform_paths.insert(ty, Some(String::new()));
            for (name, member_ty) in &fields {
                self.note_uniform_paths(*member_ty, format!("{name}."), 0);
            }
            for (name, member_ty) in &fields {
                let member_ty = *member_ty;
                if self.is_unit(member_ty) {
                    continue;
                }
                let what = || match closure {
                    true => msg!["it captures ", code(name)],
                    false => msg!["its uniforms include ", field(format!("{struct_name}.{name}"))],
                };
                // Captured without copying: a reference, which the GPU can't be given.
                if let Ty::Ref { to, .. } = *self.mir.types.get(member_ty) {
                    let index = fields.iter().position(|(n, _)| n == name).expect("its own field");
                    let read = self.copies_read(index, name, to);
                    let hint = match &read[..] {
                        [] => msg![
                            "a shader closure's captures are its uniforms: capture copies of what it uses, ",
                            code(format!("let x = {name}.x;")),
                            " before the closure"
                        ],
                        paths => {
                            let lets: Vec<String> = (paths.iter())
                                .map(|p| format!("let {} = {p};", p.rsplit('.').next().unwrap_or(p)))
                                .collect();
                            msg![
                                "a shader closure's captures are its uniforms, and it reads ",
                                code(paths.join(", ")),
                                ": capture copies of those, ",
                                code(lets.join(" ")),
                                " before the closure, and use them in it"
                            ]
                        }
                    };
                    return Err(problem(
                        msg!["a shader can't capture ", code(name), ", a ", self.ty_part(to), ", which doesn't copy"],
                        what(),
                        Some(hint),
                    ));
                }
                // All declared below, in one buffer.
                if !self.is_handle(member_ty) && !self.uniform_fits(member_ty, 0) {
                    return Err(self.unfit(span, what(), member_ty));
                }
            }
        }
        self.wgsl_uniforms(uniforms).map_err(|no| no.at(span, "its uniforms".into()))
    }

    /// What's bound apart from the uniform buffer: a texture or a storage buffer.
    pub(crate) fn is_handle(&self, ty: TypeId) -> bool {
        self.is_lang(ty, LangStruct::Texture) || self.is_lang(ty, LangStruct::Buffer)
    }

    /// The error for uniform `what`, of type `ty`, which the host can't set.
    fn unfit(&self, span: &som_base::Span, what: Message, ty: TypeId) -> Problem {
        Problem {
            message: msg!["a uniform can't be a ", self.ty_part(ty)],
            span: span.clone(),
            label: msg![what, ", a ", self.ty_part(ty)],
            hint: Some(msg![
                "a uniform is a ",
                code("f32"),
                ", ",
                code("i32"),
                ", ",
                code("u32"),
                ", ",
                code("bool"),
                ", a ",
                code("Vector"),
                " of 2 to 4 of those, an array of numbers or vectors, a ",
                code("Texture"),
                ", or a struct of the program's own holding those"
            ]),
        }
    }

    /// Uniforms `ty`'s declarations, and each of its members made from them.
    ///
    /// Every uniform but a texture is a member of one struct, `SomUniforms`, in one buffer
    /// (binding 0), in the order `som::gpu::each_uniform` gives them to the host, which lays
    /// them out by WGSL's rules for a uniform buffer: each at the next multiple of its
    /// alignment, 4 for a number, 8 for a `vec2`, 16 for a `vec3` or `vec4`. An array's items
    /// are 16 bytes apart, so one of numbers or `vec2`s is of `vec4`s, using the first of
    /// each. A `bool` is a `u32`. The sampler is binding 1, and each texture the next, in
    /// order.
    fn wgsl_uniforms(&mut self, ty: Option<TypeId>) -> Result<(String, Vec<String>), NoGpu> {
        let mut w = Wgsl::default();
        let mut made = Vec::new();
        if let Some(ty) = ty {
            let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
                unreachable!("uniforms are a struct")
            };
            for (name, field) in self.mir.struct_def(def, &args).fields.clone() {
                if self.is_unit(field) {
                    continue;
                }
                if let Some(value) = self.wgsl_leaf(&mut w, field, &name, 1)? {
                    made.push(value);
                }
            }
        }
        let mut globals = String::new();
        if !w.members.is_empty() {
            globals.push_str(&self.struct_text("SomUniforms", &w.members));
            globals.push_str("@group(0) @binding(0) var<uniform> som_u: SomUniforms;\n");
        }
        if !w.bound.is_empty() {
            globals.push_str(&format!("@group(0) @binding(1) var {}: sampler;\n", crate::body::SAMPLER));
        }
        for (i, declared) in w.bound.iter().enumerate() {
            globals.push_str(&format!("@group(0) @binding({}) {declared};\n", i + 2));
        }
        self.bindings = w.bound.len();
        Ok((globals, made))
    }

    /// Uniform `ty` at `path`, `depth` structs in: its members added to `w`, and the value
    /// made from them, or `None` for a texture.
    fn wgsl_leaf(&mut self, w: &mut Wgsl, ty: TypeId, path: &str, depth: usize) -> Result<Option<String>, NoGpu> {
        if self.is_lang(ty, LangStruct::Texture) {
            let name = self.unique(format!("tex_{}", ident(path)));
            self.textures.insert(path.to_string(), name.clone());
            w.bound.push(format!("var {name}: texture_2d<f32>"));
            return Ok(None);
        }
        // A storage buffer: used through a pointer to it.
        if self.is_lang(ty, LangStruct::Buffer) {
            let name = self.unique(format!("buf_{}", ident(path)));
            self.textures.insert(path.to_string(), name.clone());
            w.bound.push(format!("var<storage, read> {name}: array<u32>"));
            return Ok(None);
        }
        // A struct of the program's own: field by field, as `each_uniform` goes into it.
        if self.own_struct(ty) && depth < 16 {
            let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
                unreachable!("own structs are structs")
            };
            let fields = self.mir.struct_def(def, &args).fields.clone();
            let mut made = std::collections::HashMap::new();
            for (i, (name, field)) in fields.into_iter().enumerate() {
                if self.is_unit(field) {
                    continue;
                }
                // As the texture's place is read: by the fields' own names.
                let at = format!("{path}.{name}");
                if let Some(value) = self.wgsl_leaf(w, field, &at, depth + 1)? {
                    made.insert(i, value);
                }
            }
            let name = self.ty(ty)?;
            let parts: Vec<String> = (self.members(ty)?.into_iter())
                .map(|(_, _, i)| made.remove(&i).expect("each member is made"))
                .collect();
            return Ok(Some(match parts.is_empty() {
                true => format!("{name}(false)"),
                false => format!("{name}({})", parts.join(", ")),
            }));
        }
        let member = w.member(path);
        let at = format!("som_u.{member}");
        if self.scalar(ty)? == Some(Scalar::Bool) {
            w.members.push(self.member_line(&member, "u32"));
            return Ok(Some(format!("({at} != 0u)")));
        }
        // A matrix: its columns 16 bytes apart, as `vec4`s, each made as long as its own.
        if self.is_lang(ty, LangStruct::Matrix)
            && let Ty::Struct(def, args) = self.mir.types.get(ty).clone()
        {
            let columns = self.mir.struct_def(def, &args).fields[0].1;
            let Ty::Array { element, len } = self.mir.types.get(columns).clone() else {
                unreachable!("a matrix's columns are an array")
            };
            let n = self.mir.types.len(len);
            let rows = self.native_vector(element).map_or(4, |(_, rows)| rows);
            w.members.push(self.member_line(&member, &format!("array<vec4<f32>, {n}>")));
            let swizzle = ["", "", ".xy", ".xyz", ""][rows as usize];
            let items: Vec<String> = (0..n).map(|i| format!("{at}[{i}]{swizzle}")).collect();
            let column_array = self.ty(columns)?;
            let name = self.ty(ty)?;
            return Ok(Some(format!("{name}({column_array}({}))", items.join(", "))));
        }
        if let Ty::Array { element, len } = self.mir.types.get(ty).clone()
            && self.native_vector(ty).is_none()
        {
            let n = self.mir.types.len(len);
            let (scalar, size) = match (self.scalar(element)?, self.native_vector(element)) {
                (Some(scalar), _) => (scalar, 1),
                (None, Some(vector)) => vector,
                _ => unreachable!("uniforms that fit are arrays of numbers or vectors"),
            };
            // `vec3`s and `vec4`s are 16 bytes apart as they are.
            if size >= 3 {
                let native = self.ty(ty)?;
                w.members.push(self.member_line(&member, &native));
                return Ok(Some(at));
            }
            let slot = scalar.vector(4);
            w.members.push(self.member_line(&member, &format!("array<{slot}, {n}>")));
            let swizzle = if size == 1 { "x" } else { "xy" };
            let items: Vec<String> = (0..n).map(|i| format!("{at}[{i}].{swizzle}")).collect();
            let native = self.ty(ty)?;
            return Ok(Some(format!("{native}({})", items.join(", "))));
        }
        let native = self.ty(ty)?;
        w.members.push(self.member_line(&member, &native));
        Ok(Some(at))
    }

    /// For capture `index` of the closure being compiled, `name`, a `to` that doesn't copy:
    /// the parts of it the closure reads that do copy, as written, `g.camera`. Each is the
    /// shortest path of fields to a value that copies.
    fn copies_read(&self, index: usize, name: &str, to: TypeId) -> Vec<String> {
        let Some(body) = &self.mir.funcs[self.entry].body else {
            return Vec::new();
        };
        let env = Local::new(1);
        let mut found: Vec<String> = Vec::new();
        let mut note = |place: &Place| {
            let p = &place.projection;
            let reads_it = place.local == env
                && matches!(p.get(..3), Some([Projection::Deref, Projection::Field(k), Projection::Deref]) if *k == index);
            if !reads_it {
                return;
            }
            let mut ty = to;
            let mut path = name.to_string();
            for projection in &p[3..] {
                let Projection::Field(f) = *projection else {
                    return;
                };
                let Ty::Struct(def, args) = self.mir.types.get(ty) else {
                    return;
                };
                let s = self.mir.struct_def(*def, args);
                let Some((field_name, field_ty)) = s.fields.get(f) else {
                    return;
                };
                path = format!("{path}.{field_name}");
                ty = *field_ty;
                if self.mir.is_copy(ty) {
                    if !found.contains(&path) {
                        found.push(path);
                    }
                    return;
                }
            }
        };
        for block in body.blocks.iter() {
            for stmt in &block.stmts {
                if let StatementKind::Assign(place, rvalue) = &stmt.kind {
                    note(place);
                    for read in rvalue.places() {
                        note(read);
                    }
                }
            }
            if let TerminatorKind::Branch { condition, .. } = &block.terminator.kind
                && let Some(place) = condition.place()
            {
                note(place);
            }
        }
        found
    }

    /// Notes struct `ty`, at `prefix` among the uniforms, and those it holds, if it's one of
    /// the program's own: where their textures are read from.
    fn note_uniform_paths(&mut self, ty: TypeId, prefix: String, depth: usize) {
        if !self.own_struct(ty) || depth >= 16 {
            return;
        }
        match self.uniform_paths.get(&ty) {
            Some(Some(already)) if *already != prefix => {
                self.uniform_paths.insert(ty, None);
            }
            Some(_) => {}
            None => {
                self.uniform_paths.insert(ty, Some(prefix.clone()));
            }
        }
        let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
            return;
        };
        for (name, field) in self.mir.struct_def(def, &args).fields.clone() {
            self.note_uniform_paths(field, format!("{prefix}{name}."), depth + 1);
        }
    }

    /// A struct of the program's own, as `@leaves` and `each_uniform` go into: not generic,
    /// not the standard library's, not an enum.
    fn own_struct(&self, ty: TypeId) -> bool {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            return false;
        };
        if !args.is_empty() || self.mir.lang_structs.contains_key(def) {
            return false;
        }
        let s = self.mir.struct_def(*def, args);
        !s.name.starts_with("som::") && s.name != "closure" && s.variants.is_none() && !s.handle
    }

    /// Whether `ty` can be a uniform the host sets: a number, `bool` or vector of numbers, an
    /// array of numbers or `f32` vectors, or a struct of those (set by each of its fields).
    /// Textures and buffers below the top too, each bound on its own.
    fn uniform_fits(&self, ty: TypeId, depth: usize) -> bool {
        if let Ok(Some(_)) = self.scalar(ty) {
            return true;
        }
        if let Some((scalar, _)) = self.native_vector(ty) {
            return matches!(scalar, Scalar::Float | Scalar::Int | Scalar::Uint);
        }
        // Below the top, where it's a uniform of its own.
        if self.is_handle(ty) {
            return depth > 0;
        }
        if self.is_lang(ty, LangStruct::Matrix) {
            return true;
        }
        match self.mir.types.get(ty).clone() {
            // Of numbers (more than 4: fewer is a vector) or of `f32` vectors.
            Ty::Array { element, .. } => {
                matches!(self.scalar(element), Ok(Some(Scalar::Float | Scalar::Int | Scalar::Uint)))
                    || matches!(self.native_vector(element), Some((Scalar::Float, _)))
            }
            Ty::Struct(def, args) if depth < 16 && !self.mir.lang_structs.contains_key(&def) => {
                // As `@leaves` goes into them: the program's own, not generic.
                let s = self.mir.struct_def(def, &args);
                args.is_empty()
                    && !s.name.starts_with("som::")
                    && s.variants.is_none()
                    && !s.handle
                    && (s.fields.iter()).all(|(_, t)| self.is_unit(*t) || self.uniform_fits(*t, depth + 1))
            }
            _ => false,
        }
    }
}

//! `@vertex` shaders, and what they pass to the `@fragment` shader drawn with them.
//!
//! A vertex shader takes a struct of the corner's numbers (each field a vertex attribute,
//! `@location(i)` in field order, read from the mesh's buffer as the CPU lays the struct out)
//! and may take a `&` to a struct of uniforms. It gives a `gpu::Vertex<S>`: the clip position,
//! and an `S` of the program's own, or a number or vector of them, passed on whole as
//! `@location(0)`. Each of a struct `S`'s fields is `@location(i)` of `SomVaryings`,
//! blended between the corners for each pixel (an integer is taken from the first, `flat`).
//! The fragment shader taking that `S` is made from the same `SomVaryings`, so the two
//! modules agree on it without knowing of each other.

use som_base::message::code;
use som_base::{Message, msg};
use som_lang::LangStruct;
use som_mir::*;

use crate::Translator;
use crate::entry::Main;
use crate::error::Problem;
use crate::types::Scalar;

/// What's passed from the vertex shader to the fragment shader.
pub(crate) const VARYINGS: &str = "SomVaryings";

/// At most this many fields in what's passed on, or attributes read: WebGPU's least limit.
const MOST: usize = 16;

/// What a vertex shader's parameter is given.
enum Param {
    Attributes(TypeId),
    Uniforms,
    Unit,
}

impl Translator<'_> {
    pub(crate) fn vertex(&mut self) -> Main {
        match self.try_vertex() {
            Ok(main) => main,
            Err(problem) => {
                self.fail(self.entry, problem);
                Main::default()
            }
        }
    }

    fn try_vertex(&mut self) -> Result<Main, Problem> {
        let func = &self.mir.funcs[self.entry];
        let span = crate::head(&func.span);
        let body = func.body.as_ref().expect("a shader has a body");
        let shape = || {
            Some(msg![
                "a vertex shader takes a struct of the corner's numbers, may take a ",
                code("&"),
                " to a struct of uniforms, and gives a ",
                code("gpu::Vertex<S>"),
                ": where the corner lands, and an ",
                code("S"),
                " for the fragment shader"
            ])
        };
        let ret = func.ret;
        let Some(out) = self.vertex_out(ret) else {
            return Err(Problem {
                message: msg!["a vertex shader gives a ", code("gpu::Vertex<S>"), ", not a ", self.ty_part(ret)],
                span,
                label: msg!["it returns a ", self.ty_part(ret)],
                hint: shape(),
            });
        };
        let mut params = Vec::new();
        let mut uniforms = None;
        let mut attributes = None;
        for (i, &param) in func.params.iter().enumerate() {
            let param_span = body.locals[Local::new(i + 1)].span.clone();
            let plain_struct = |t: &Translator, ty: TypeId| {
                matches!(t.mir.types.get(ty), Ty::Struct(def, args) if t.mir.struct_def(*def, args).variants.is_none())
            };
            params.push(match self.mir.types.get(param).clone() {
                _ if self.is_unit(param) => Param::Unit,
                Ty::Ref { mutable: false, to } if plain_struct(self, to) && uniforms.replace(to).is_none() => {
                    Param::Uniforms
                }
                _ if plain_struct(self, param) && attributes.replace(param).is_none() => Param::Attributes(param),
                _ => {
                    return Err(Problem {
                        message: msg!["a vertex shader can't take a ", self.ty_part(param)],
                        span: param_span,
                        label: msg!["this is a ", self.ty_part(param)],
                        hint: shape(),
                    });
                }
            });
        }

        self.uniforms = uniforms;
        let mut globals = self.uniform_fields(uniforms, &span)?;
        globals.push_str(&self.varyings_struct(out, &span)?);

        // The attributes: each a parameter of `vs_main`, made into the struct.
        let mut inputs = Vec::new();
        let mut args = Vec::new();
        for (i, param) in params.into_iter().enumerate() {
            args.push(match param {
                Param::Unit => continue,
                // Read where they are, not passed.
                Param::Uniforms => {
                    self.entry_bound.push((i, String::new()));
                    continue;
                }
                Param::Attributes(ty) => {
                    let what = |t: &Translator| msg!["it takes a ", t.ty_part(ty)];
                    let fields = self.leaf_fields(ty, |t| what(t), &span, "a vertex attribute")?;
                    let mut parts = Vec::new();
                    for (i, (native, _)) in fields.iter().enumerate() {
                        inputs.push(format!("@location({i}) a{i}: {native}"));
                        parts.push(format!("a{i}"));
                    }
                    let name = self.ty(ty).map_err(|no| no.at(&span, what(self)))?;
                    format!("{name}({})", parts.join(", "))
                }
            });
        }

        let shader = self.func_name(self.entry, &self.entry_bound.clone());
        let vertex = self.ty(ret).map_err(|no| no.at(&span, "it".into()))?;
        let position = self.field_member(ret, "position");
        let passed_member = self.field_member(ret, "passed");
        let count = self.leaf_fields(out, |t| msg!["it gives a ", t.ty_part(out)], &span, "passed to the fragment shader")?.len();
        let mut passed = vec![format!("r.{position}")];
        if self.passed_whole(out).is_some() {
            passed.push(format!("r.{passed_member}"));
        } else {
            for i in 0..count {
                let member = self.member_name(out, self.field_index(out, i)).expect("checked above");
                passed.push(format!("r.{passed_member}.{member}"));
            }
        }
        let main = format!(
            "@vertex\nfn vs_main({}) -> {VARYINGS} {{\n    let r: {vertex} = {shader}({});\n    return {VARYINGS}({});\n}}\n",
            inputs.join(", "),
            args.join(", "),
            passed.join(", "),
        );
        Ok(Main { globals, main })
    }

    /// `S`, of `gpu::Vertex<S>`: what a vertex shader giving `ty` passes on.
    fn vertex_out(&self, ty: TypeId) -> Option<TypeId> {
        if !self.is_lang(ty, LangStruct::Vertex) {
            return None;
        }
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            return None;
        };
        let s = self.mir.struct_def(*def, args);
        s.fields.iter().find(|(name, _)| name == "passed").map(|&(_, t)| t)
    }

    /// The member of struct `ty` that its field `name` is.
    fn field_member(&mut self, ty: TypeId, name: &str) -> String {
        let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
            unreachable!("a struct")
        };
        let index = (self.mir.struct_def(def, &args).fields.iter())
            .position(|(n, _)| n == name)
            .expect("the field is there");
        self.member_name(ty, index).expect("a field of numbers")
    }

    /// The index among the MIR fields of `ty`'s `i`th field that isn't `()`.
    fn field_index(&self, ty: TypeId, i: usize) -> usize {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            unreachable!("a struct")
        };
        (self.mir.struct_def(*def, args).fields.iter().enumerate())
            .filter(|(_, (_, t))| !self.is_unit(*t))
            .nth(i)
            .map(|(index, _)| index)
            .expect("counted above")
    }

    /// `ty`'s fields, each a number or vector of numbers (`f32`, `i32`, `u32`): its WGSL type
    /// and whether it's an integer. `what` says where `ty` is, for errors; `one` is what a
    /// field is called there.
    fn leaf_fields(
        &mut self,
        ty: TypeId,
        what: impl Fn(&Translator) -> Message,
        span: &som_base::Span,
        one: &str,
    ) -> Result<Vec<(String, bool)>, Problem> {
        if let Some(scalar) = self.passed_whole(ty) {
            let native = self.ty(ty).map_err(|no| no.at(span, what(self)))?;
            return Ok(vec![(native, scalar != Scalar::Float)]);
        }
        let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
            unreachable!("a struct")
        };
        let fields = self.mir.struct_def(def, &args).fields.clone();
        let mut leaves = Vec::new();
        for (name, field) in fields {
            if self.is_unit(field) {
                continue;
            }
            let scalar = match (self.scalar(field), self.native_vector(field)) {
                (Ok(Some(scalar)), _) | (_, Some((scalar, _))) => Some(scalar),
                _ => None,
            };
            if scalar.is_none_or(|s| s == Scalar::Bool) {
                return Err(Problem {
                    message: msg![code(format!("{}.{name}", self.display(ty))), " can't be ", one],
                    span: span.clone(),
                    label: what(self),
                    hint: Some(msg![
                        "each field is a number or a vector of them: ",
                        code("f32"),
                        ", ",
                        code("i32"),
                        ", ",
                        code("u32"),
                        ", ",
                        code("Vec3"),
                        "…"
                    ]),
                });
            }
            let native = self.ty(field).map_err(|no| no.at(span, what(self)))?;
            leaves.push((native, scalar != Some(Scalar::Float)));
        }
        if leaves.len() > MOST {
            return Err(Problem {
                message: msg![code(self.display(ty)), " has more than ", MOST.to_string(), " fields"],
                span: span.clone(),
                label: what(self),
                hint: Some("WebGPU passes at most 16 from one stage to the next".into()),
            });
        }
        Ok(leaves)
    }

    /// `SomVaryings`, declared for `out`: the position, then each of `out`'s fields.
    pub(crate) fn varyings_struct(&mut self, out: TypeId, span: &som_base::Span) -> Result<String, Problem> {
        let leaves = self.leaf_fields(out, |t| msg!["what's passed on, ", t.ty_part(out)], span, "passed to the fragment shader")?;
        let mut lines = vec!["    @builtin(position) position: vec4<f32>,".to_string()];
        for (i, (native, integer)) in leaves.iter().enumerate() {
            let flat = if *integer { " @interpolate(flat)" } else { "" };
            lines.push(format!("    @location({i}){flat} v{i}: {native},"));
        }
        Ok(format!("struct {VARYINGS} {{\n{}\n}}\n", lines.join("\n")))
    }

    /// A number or vector of numbers (`f32`, `i32`, `u32`): passed on whole, as one field, not
    /// field by field like a struct of the program's. The kind of number it is.
    pub(crate) fn passed_whole(&self, ty: TypeId) -> Option<Scalar> {
        let scalar = match self.scalar(ty) {
            Ok(Some(scalar)) => Some(scalar),
            _ => self.native_vector(ty).map(|(scalar, _)| scalar),
        };
        scalar.filter(|&s| s != Scalar::Bool)
    }

    /// `out`, made from `SomVaryings` `v` in a fragment shader.
    pub(crate) fn from_varyings(&mut self, out: TypeId, span: &som_base::Span) -> Result<String, Problem> {
        if self.passed_whole(out).is_some() {
            return Ok("v.v0".into());
        }
        let count = self.leaf_fields(out, |t| msg!["it takes a ", t.ty_part(out)], span, "passed to the fragment shader")?.len();
        let parts: Vec<String> = (0..count).map(|i| format!("v.v{i}")).collect();
        let name = self.ty(out).map_err(|no| no.at(span, "it".into()))?;
        Ok(format!("{name}({})", parts.join(", ")))
    }
}

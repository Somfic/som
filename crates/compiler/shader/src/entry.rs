//! What's around a shader in GLSL: its inputs, uniforms and output, and the `main` that calls
//! it. They're named as raylib's own shaders name them, so it goes with raylib's vertex
//! shader and its uniforms are set by name.

use som_base::message::{code, field};
use som_base::{Message, msg};
use som_lang::LangStruct;
use som_mir::*;

use crate::Translator;
use crate::error::Problem;
use crate::names::{ident, is_reserved};
use crate::types::Scalar;

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

        let mut globals = String::from(
            "in vec2 fragTexCoord;\nin vec4 fragColor;\nout vec4 finalColor;\n",
        );
        let mut uniform_args = Vec::new();
        self.uniforms = uniforms;
        if let Some(ty) = uniforms {
            let Ty::Struct(def, args) = self.mir.types.get(ty).clone() else {
                unreachable!("uniforms are a struct")
            };
            let fields = self.mir.struct_def(def, &args).fields.clone();
            let struct_name = self.display(ty);
            for (name, member_ty) in &fields {
                let member_ty = *member_ty;
                if self.is_unit(member_ty) {
                    continue;
                }
                let what = || msg![field(format!("{struct_name}.{name}"))];
                if ident(name) != *name || is_reserved(name) {
                    return Err(problem(
                        msg!["a uniform can't be named ", code(name)],
                        msg!["its uniforms include ", what()],
                        Some("GLSL has the name; rename the field".into()),
                    ));
                }
                // A texture is a uniform of its own, used where it's named.
                if self.is_lang(member_ty, LangStruct::Texture) {
                    globals.push_str(&format!("uniform sampler2D {name};\n"));
                    continue;
                }
                // What `som::gpu::Uniform` can hold, for the host to set.
                let native = match (self.scalar(member_ty), self.vector(member_ty)) {
                    (Ok(Some(scalar)), _) if scalar != Scalar::Uint => scalar.name().to_string(),
                    (_, Some((Scalar::Float, n))) => Scalar::Float.vector(n),
                    _ => {
                        return Err(problem(
                            msg!["a uniform can't be a ", self.ty_part(member_ty)],
                            msg!["its uniforms include ", what(), ", a ", self.ty_part(member_ty)],
                            Some(msg![
                                "a uniform is a ",
                                code("f32"),
                                ", ",
                                code("i32"),
                                ", ",
                                code("bool"),
                                ", or a ",
                                code("Vector"),
                                " of 2 to 4 ",
                                code("f32"),
                                "s, or a ",
                                code("Texture")
                            ]),
                        ));
                    }
                };
                globals.push_str(&format!("uniform {native} {name};\n"));
                uniform_args.push(self.from_native(name, member_ty));
            }
        }

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
                    for (_, member_ty, index) in members {
                        let native = match fields[index].0.as_str() {
                            "uv" => "fragTexCoord",
                            "color" => "fragColor",
                            "position" => "gl_FragCoord",
                            other => unreachable!("`Fragment` has no `{other}`"),
                        };
                        parts.push(self.from_native(native, member_ty));
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
        let colour = self.ty(func.ret).map_err(|no| no.at(&span, "it".into()))?;
        let main = format!(
            "void main() {{\n    {colour} colour = {shader}({});\n    finalColor = {};\n}}\n",
            args.join(", "),
            self.to_native("colour", func.ret),
        );
        Ok(Main { globals, main })
    }

    /// GLSL value `native`, a scalar or `vecN`, as a value of `ty`: the same, since a
    /// `Vector` is GLSL's own.
    fn from_native(&mut self, native: &str, _ty: TypeId) -> String {
        native.to_string()
    }

    /// `value`, a `Vector`, as GLSL's own vector: it already is.
    fn to_native(&mut self, value: &str, _ty: TypeId) -> String {
        value.to_string()
    }
}

//! Types on the GPU: what each of the program's is in WGSL, and its values.

use som_base::message::{code, ty as ty_part};
use som_base::msg;
use som_lang::LangStruct;
use som_mir::*;

use crate::Translator;
use crate::error::NoGpu;
use crate::names::{ident, member};

/// A WGSL scalar: what a number or `bool` is on the GPU.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Scalar {
    Bool,
    Int,
    Uint,
    Float,
}

impl Scalar {
    /// Its name in WGSL.
    pub fn name(self) -> &'static str {
        match self {
            Scalar::Bool => "bool",
            Scalar::Int => "i32",
            Scalar::Uint => "u32",
            Scalar::Float => "f32",
        }
    }

    /// The vector of `n` of them: `vec4<f32>`.
    pub fn vector(self, n: u64) -> String {
        format!("vec{n}<{}>", self.name())
    }
}

impl Translator<'_> {
    pub(crate) fn is_unit(&self, ty: TypeId) -> bool {
        matches!(self.mir.types.get(ty), Ty::Unit)
    }

    /// `ty` as messages name it: without the modules, `Vector<4, f32>`.
    pub(crate) fn display(&self, ty: TypeId) -> String {
        crate::short(&self.mir.display_ty(ty))
    }

    /// The scalar `ty` is on the GPU, if it's a number or `bool` it has.
    pub(crate) fn scalar(&self, ty: TypeId) -> Result<Option<Scalar>, NoGpu> {
        Ok(Some(match self.mir.types.get(ty) {
            Ty::Bool => Scalar::Bool,
            Ty::Float(FloatTy::F32) => Scalar::Float,
            Ty::Int(IntTy::I32) => Scalar::Int,
            // Indexes are `usize`; on the GPU they're 32 bits.
            Ty::Int(IntTy::U32 | IntTy::USize) => Scalar::Uint,
            Ty::Float(FloatTy::F64) => {
                return Err(NoGpu::new(
                    code("f64"),
                    Some(msg!["its numbers are 32 bits: use ", code("f32")]),
                ));
            }
            Ty::Int(int) => {
                let instead = if int.is_signed() { "i32" } else { "u32" };
                return Err(NoGpu::new(
                    code(int.name()),
                    Some(msg!["its integers are 32 bits: use ", code(instead)]),
                ));
            }
            _ => return Ok(None),
        }))
    }

    /// `Vector<N, T>` with `N` from 2 to 4 and `T` a scalar: what WGSL has a vector type for.
    pub(crate) fn vector(&self, ty: TypeId) -> Option<(Scalar, u64)> {
        let Ty::Struct(def, args) = self.mir.types.get(ty) else {
            return None;
        };
        if self.mir.lang_structs.get(def) != Some(&LangStruct::Vector) {
            return None;
        }
        let s = self.mir.struct_def(*def, args);
        self.small_array(s.fields.first()?.1)
    }

    /// `[T; N]` with `N` from 2 to 4 and `T` a scalar.
    fn small_array(&self, ty: TypeId) -> Option<(Scalar, u64)> {
        let Ty::Array { element, len } = self.mir.types.get(ty) else {
            return None;
        };
        let n = self.mir.types.len(*len);
        let scalar = self.scalar(*element).ok()??;
        (2..=4).contains(&n).then_some((scalar, n))
    }

    /// What's WGSL's own vector: a small array of numbers or `bool`s, `[f32; 4]` as `vec4<f32>`,
    /// and so a `Vector`, which is one. Indexing and building it are the same as an array's,
    /// and the shader's inputs and output need no converting.
    pub(crate) fn native_vector(&self, ty: TypeId) -> Option<(Scalar, u64)> {
        self.small_array(ty).or_else(|| self.vector(ty))
    }

    pub(crate) fn is_lang(&self, ty: TypeId, lang: LangStruct) -> bool {
        matches!(self.mir.types.get(ty), Ty::Struct(def, _) if self.mir.lang_structs.get(def) == Some(&lang))
    }

    /// The WGSL type of `ty`, declaring the structs it needs first.
    pub(crate) fn ty(&mut self, ty: TypeId) -> Result<String, NoGpu> {
        if let Some(scalar) = self.scalar(ty)? {
            return Ok(scalar.name().into());
        }
        if let Some((scalar, n)) = self.native_vector(ty) {
            return Ok(scalar.vector(n));
        }
        if self.is_lang(ty, LangStruct::Texture) {
            return Ok("texture_2d<f32>".into());
        }
        // WGSL can't pass a pointer to a buffer to a function (naga doesn't take
        // `unrestricted_pointer_parameters` yet): each buffer is read where it's named.
        if self.is_lang(ty, LangStruct::Buffer) {
            return Err(NoGpu::new(
                msg![code("gpu::Buffer"), "s passed around"],
                Some(msg![
                    "read a buffer's items where it's captured, ",
                    code("cells.get(i)"),
                    ", and pass those"
                ]),
            ));
        }
        match self.mir.types.get(ty).clone() {
            Ty::Unit => Ok("void".into()),
            Ty::Struct(..) | Ty::Optional(_) | Ty::Range(_) => self.struct_ty(ty),
            Ty::Array { element, len } => {
                let n = self.mir.types.len(len);
                if n == 0 {
                    return Err(NoGpu::new("empty arrays", None));
                }
                Ok(format!("array<{}, {n}>", self.ty(element)?))
            }
            Ty::Str | Ty::CStr => Err(NoGpu::new("text", None)),
            Ty::Char => Err(NoGpu::new(code("char"), None)),
            Ty::Pointer | Ty::Ptr(_) => Err(NoGpu::new("pointers", None)),
            Ty::Slice(_) => Err(NoGpu::new(
                "slices",
                Some("use an array, whose length is known while compiling".into()),
            )),
            Ty::Ref { to, .. } if matches!(self.mir.types.get(to), Ty::Slice(_) | Ty::Str) => {
                self.ty(to)
            }
            Ty::Ref { .. } => Err(NoGpu::new(
                "references to keep",
                Some(msg!["a shader can pass a ", code("&"), " to a function, and nothing else"]),
            )),
            Ty::Fn { .. } | Ty::FnItem(..) | Ty::Closure(..) => Err(NoGpu::new(
                "function values",
                Some("call the function by its name".into()),
            )),
            other => unreachable!("lowered code has concrete types: {other:?}"),
        }
    }

    /// `ty`'s members, as WGSL has them: each one's name, type and index among the MIR
    /// fields. `()` members aren't there.
    pub(crate) fn members(&mut self, ty: TypeId) -> Result<Vec<(String, TypeId, usize)>, NoGpu> {
        let fields: Vec<(String, TypeId)> = match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => {
                let s = self.mir.struct_def(def, &args);
                if s.handle {
                    return Err(NoGpu::new(
                        msg![code(&s.name), ", which only C sees inside"],
                        None,
                    ));
                }
                s.fields.clone()
            }
            Ty::Optional(inner) => vec![("has".into(), self.mir.types.bool), ("value".into(), inner)],
            Ty::Range(int) => vec![("start".into(), int), ("end".into(), int)],
            other => unreachable!("only structs have members: {other:?}"),
        };
        // The uniforms' textures and buffers are globals of their own.
        // Numbered first: an index is among all the MIR fields.
        let textures_apart = self.uniform_paths.contains_key(&ty);
        Ok(fields
            .into_iter()
            .enumerate()
            .filter(|(_, (_, t))| !(textures_apart && self.is_handle(*t)))
            .filter(|(_, (_, t))| !self.is_unit(*t))
            .map(|(i, (name, t))| (member(&name), t, i))
            .collect())
    }

    /// The name of WGSL member `index` of `ty`.
    pub(crate) fn member_name(&mut self, ty: TypeId, index: usize) -> Result<String, NoGpu> {
        let members = self.members(ty)?;
        Ok((members.into_iter())
            .find(|(_, _, i)| *i == index)
            .map(|(name, ..)| name)
            .expect("a field that's read is there"))
    }

    fn struct_ty(&mut self, ty: TypeId) -> Result<String, NoGpu> {
        if let Some(name) = self.struct_names.get(&ty) {
            return Ok(name.clone());
        }
        let base = match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => ident(&self.mir.struct_def(def, &args).name),
            Ty::Optional(inner) => format!("Optional_{}", ident(&self.ty(inner)?)),
            Ty::Range(int) => format!("Range_{}", ident(&self.ty(int)?)),
            _ => unreachable!("only these are structs"),
        };
        if self.is_lang(ty, LangStruct::String) {
            return Err(NoGpu::new("text", None));
        }
        let mut lines = Vec::new();
        for (name, member_ty, _) in self.members(ty)? {
            if self.is_handle(member_ty) {
                return Err(NoGpu::new(
                    msg!["a ", code(self.display(member_ty)), " inside a struct"],
                    Some(msg![
                        "a texture is a field of the shader's uniforms, and passed to functions as a ",
                        code("&Texture")
                    ]),
                ));
            }
            // Said of the struct: what it holds is likely out of sight.
            let t = self.ty(member_ty).map_err(|inner| {
                let mut hint = msg![code(self.display(ty)), " holds ", inner.what];
                if let Some(more) = inner.hint {
                    hint.push(msg!["; ", more]);
                }
                NoGpu::new(code(self.display(ty)), Some(hint))
            })?;
            lines.push(self.member_line(&name, &t));
        }
        // WGSL has no empty structs.
        if lines.is_empty() {
            lines.push(self.member_line("nothing", "bool"));
        }
        let name = self.unique(base);
        let declared = self.struct_text(&name, &lines);
        self.structs.push(declared);
        self.struct_names.insert(ty, name.clone());
        Ok(name)
    }

    /// Member `name` of type `ty`, as a line of a struct.
    pub(crate) fn member_line(&self, name: &str, ty: &str) -> String {
        format!("    {name}: {ty},")
    }

    /// Struct `name` with member `lines`, declared.
    pub(crate) fn struct_text(&self, name: &str, lines: &[String]) -> String {
        format!("struct {name} {{\n{}\n}}\n", lines.join("\n"))
    }

    /// `ty`'s value with every byte zero: `0`, `false`, or a struct or array of those.
    pub(crate) fn zero(&mut self, ty: TypeId) -> Result<String, NoGpu> {
        match self.scalar(ty)? {
            Some(scalar) => Ok(self.zero_scalar(scalar).into()),
            // WGSL's constructors with nothing given are zero.
            None => Ok(format!("{}()", self.ty(ty)?)),
        }
    }

    /// Constant `c` in WGSL.
    pub(crate) fn constant(&mut self, c: &Const) -> Result<String, NoGpu> {
        match &c.kind {
            ConstKind::Int(n) => Ok(match self.scalar(c.ty)? {
                Some(Scalar::Uint) => format!("{}u", *n as u32),
                Some(Scalar::Float) => float(*n as f64)?,
                _ => {
                    let n = *n as i32;
                    match n {
                        i32::MIN => "(-2147483647 - 1)".into(),
                        n if n < 0 => format!("({n})"),
                        n => n.to_string(),
                    }
                }
            }),
            ConstKind::Float(x) => float(*x),
            ConstKind::Bool(b) => Ok(b.to_string()),
            ConstKind::Unit => Ok(String::new()),
            ConstKind::Zeroed => self.zero(c.ty),
            ConstKind::Aggregate(parts) => self.aggregate(c.ty, parts),
            ConstKind::Str(_) => Err(NoGpu::new("text", None)),
            ConstKind::Func(_) => Err(NoGpu::new(
                "function values",
                Some("call the function by its name".into()),
            )),
            ConstKind::Pending(_) => unreachable!("constants are worked out before shaders"),
        }
    }

    fn aggregate(&mut self, ty: TypeId, parts: &[Const]) -> Result<String, NoGpu> {
        // A `Vector` is its one field.
        if self.vector(ty).is_some() {
            return self.constant(&parts[0]);
        }
        let name = self.ty(ty)?;
        if matches!(self.mir.types.get(ty), Ty::Array { .. }) {
            let parts = (parts.iter())
                .map(|p| self.constant(p))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(format!("{name}({})", parts.join(", ")));
        }
        let members = self.members(ty)?;
        if members.is_empty() {
            return Ok(format!("{name}(false)"));
        }
        let parts = (members.into_iter())
            .map(|(_, _, i)| self.constant(&parts[i]))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(format!("{name}({})", parts.join(", ")))
    }

    fn zero_scalar(&self, scalar: Scalar) -> &'static str {
        match scalar {
            Scalar::Bool => "false",
            Scalar::Int => "0",
            Scalar::Uint => "0u",
            Scalar::Float => "0.0",
        }
    }

    /// The type `ty` names in messages.
    pub(crate) fn ty_part(&self, ty: TypeId) -> som_base::message::Part {
        ty_part(self.display(ty))
    }
}

/// A float literal WGSL reads: always with a `.` or an exponent.
fn float(x: f64) -> Result<String, NoGpu> {
    if !x.is_finite() {
        return Err(NoGpu::new(
            "constants that aren't finite",
            Some("work out infinity or NaN when the shader runs".into()),
        ));
    }
    let text = format!("{:?}", x as f32);
    let text = if text.contains(['.', 'e']) {
        text
    } else {
        format!("{text}.0")
    };
    Ok(match x < 0.0 {
        true => format!("({text})"),
        false => text,
    })
}

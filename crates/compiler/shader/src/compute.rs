//! Compute kernels, and storage buffers (`gpu::Buffer<T>`), in WGSL.
//!
//! A kernel runs once for each item of the buffer it fills: given the item's index (and its
//! uniforms), it gives the item. `cs_main` calls it for every index below the buffer's
//! length and stores what it gives.
//!
//! A buffer is declared as `array<u32>`, its items laid out as C lays them out, so the host
//! copies a som array in and out as it is. Each item is read and written word by word, by
//! a function made for it: `som_load_buf_particles(i * 5u)`. (Made for each buffer, since
//! WGSL can't pass a pointer to one to a function; so a buffer is read where it's captured.) So an item can only be
//! made of 4-byte numbers (`f32`, `i32`, `u32`, vectors, arrays and structs of those), which
//! C lays out with no gaps between them.

use som_base::message::code;
use som_base::msg;
use som_lang::LangStruct;
use som_mir::*;

use crate::Translator;
use crate::entry::Main;
use crate::error::{NoGpu, Problem};
use crate::names::ident;
use crate::types::Scalar;

/// The buffer a kernel fills.
pub(crate) const OUT: &str = "som_out";

/// How many threads run together: each group of the dispatch, as `cs_main` declares it.
pub const WORKGROUP: u32 = 64;

impl Translator<'_> {
    pub(crate) fn compute(&mut self) -> Main {
        match self.try_compute() {
            Ok(main) => main,
            Err(problem) => {
                self.fail(self.entry, problem);
                Main::default()
            }
        }
    }

    fn try_compute(&mut self) -> Result<Main, Problem> {
        let func = &self.mir.funcs[self.entry];
        let span = crate::head(&func.span);
        let body = func.body.as_ref().expect("a kernel has a body");
        let shape = || {
            Some(msg![
                "a kernel takes the index of the item it gives, a ",
                code("usize"),
                ", and may take a ",
                code("&"),
                " to a struct of uniforms"
            ])
        };
        let ret = func.ret;
        let item = (self.words(ret)).map_err(|no| no.at(&span, msg!["it gives a ", self.ty_part(ret)]))?;
        if item == 0 {
            return Err(Problem {
                message: "a kernel gives the item at its index".into(),
                span,
                label: "it gives nothing".into(),
                hint: shape(),
            });
        }
        let mut uniforms = None;
        let mut args = Vec::new();
        let mut index = false;
        for (i, &param) in func.params.iter().enumerate() {
            let param_span = body.locals[Local::new(i + 1)].span.clone();
            if self.is_unit(param) {
                continue;
            }
            let lent = match self.mir.types.get(param) {
                Ty::Ref { mutable: false, to }
                    if matches!(self.mir.types.get(*to), Ty::Struct(def, args)
                        if self.mir.struct_def(*def, args).variants.is_none()) =>
                {
                    Some(*to)
                }
                _ => None,
            };
            if matches!(self.mir.types.get(param), Ty::Int(IntTy::USize | IntTy::U32)) && !index {
                index = true;
                args.push(Arg::Index);
            } else if let Some(to) = lent
                && uniforms.replace(to).is_none()
            {
                args.push(Arg::Uniforms(to));
            } else {
                return Err(Problem {
                    message: msg!["a kernel can't take a ", self.ty_part(param)],
                    span: param_span,
                    label: msg!["this is a ", self.ty_part(param)],
                    hint: shape(),
                });
            }
        }

        self.uniforms = uniforms;
        let (mut globals, made) = self.uniform_fields(uniforms, &span)?;
        // The buffer filled: after every other binding.
        globals.push_str(&format!(
            "@group(0) @binding({}) var<storage, read_write> {OUT}: array<u32>;\n",
            self.bindings + 2
        ));
        let mut texts = Vec::new();
        for arg in args {
            texts.push(match arg {
                Arg::Index => "i".to_string(),
                Arg::Uniforms(ty) => {
                    let name = self.ty(ty).map_err(|no| no.at(&span, "its uniforms".into()))?;
                    match made.is_empty() {
                        true => format!("{name}(false)"),
                        false => format!("{name}({})", made.join(", ")),
                    }
                }
            });
        }
        let store = self.store(ret).map_err(|no| no.at(&span, msg!["it gives a ", self.ty_part(ret)]))?;
        let kernel = self.func_name(self.entry);
        let main = format!(
            "@compute @workgroup_size({WORKGROUP})\nfn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {{\n    \
             let i = id.x;\n    if (i >= arrayLength(&{OUT}) / {item}u) {{\n        return;\n    }}\n    \
             {store}(i * {item}u, {kernel}({}));\n}}\n",
            texts.join(", ")
        );
        Ok(Main { globals, main })
    }

    /// How many 4-byte words a `ty` takes in a buffer, as C lays it out: or why it can't be
    /// in one.
    pub(crate) fn words(&mut self, ty: TypeId) -> Result<u32, NoGpu> {
        let not = || {
            NoGpu::new(
                msg!["a buffer of ", code("bool"), "s, ", code("usize"), "s or enums"],
                Some(msg![
                    "a buffer's items are made of 4-byte numbers, ",
                    code("f32"),
                    ", ",
                    code("i32"),
                    " and ",
                    code("u32"),
                    ", laid out as the CPU has them"
                ]),
            )
        };
        if self.is_unit(ty) {
            return Ok(0);
        }
        match self.mir.types.get(ty).clone() {
            Ty::Float(FloatTy::F32) | Ty::Int(IntTy::I32 | IntTy::U32) => Ok(1),
            Ty::Array { element, len } => Ok(self.words(element)? * self.mir.types.len(len) as u32),
            Ty::Struct(def, args) => {
                let s = self.mir.struct_def(def, &args);
                if s.variants.is_some() || s.handle || self.is_handle(ty) || self.is_lang(ty, LangStruct::String) {
                    return Err(not());
                }
                let fields: Vec<TypeId> = s.fields.iter().map(|&(_, t)| t).collect();
                let mut total = 0;
                for field in fields {
                    total += self.words(field)?;
                }
                Ok(total)
            }
            _ => Err(not()),
        }
    }

    /// A `ty` read from words `at` on of buffer pointer `b`.
    fn read_words(&mut self, ty: TypeId, b: &str, at: u32) -> Result<String, NoGpu> {
        let word = |k: u32| format!("{b}[at + {k}u]");
        if let Some(scalar) = self.scalar(ty)? {
            return Ok(match scalar {
                Scalar::Float => format!("bitcast<f32>({})", word(at)),
                Scalar::Int => format!("bitcast<i32>({})", word(at)),
                _ => word(at),
            });
        }
        let name = self.ty(ty)?;
        let mut parts = Vec::new();
        match self.mir.types.get(ty).clone() {
            Ty::Array { element, len } => {
                let step = self.words(element)?;
                for k in 0..self.mir.types.len(len) as u32 {
                    parts.push(self.read_words(element, b, at + k * step)?);
                }
            }
            Ty::Struct(def, args) if self.native_vector(ty).is_none() => {
                let fields: Vec<TypeId> = self.mir.struct_def(def, &args).fields.iter().map(|&(_, t)| t).collect();
                let mut offset = at;
                let members = self.members(ty)?;
                let mut offsets = Vec::new();
                for field in &fields {
                    offsets.push(offset);
                    offset += self.words(*field)?;
                }
                for (_, member_ty, i) in members {
                    parts.push(self.read_words(member_ty, b, offsets[i])?);
                }
            }
            // A `Vector`: its array.
            Ty::Struct(def, args) => {
                let array = self.mir.struct_def(def, &args).fields[0].1;
                return self.read_words(array, b, at);
            }
            _ => unreachable!("only these have words"),
        }
        Ok(format!("{name}({})", parts.join(", ")))
    }

    /// Statements writing `value`, a `ty`, to the words from `at` on of the kernel's buffer.
    fn write_words(&mut self, ty: TypeId, value: &str, at: u32, lines: &mut Vec<String>) -> Result<(), NoGpu> {
        if let Some(scalar) = self.scalar(ty)? {
            let bits = match scalar {
                Scalar::Uint => value.to_string(),
                _ => format!("bitcast<u32>({value})"),
            };
            lines.push(format!("    {OUT}[at + {at}u] = {bits};"));
            return Ok(());
        }
        match self.mir.types.get(ty).clone() {
            Ty::Array { element, len } => {
                let step = self.words(element)?;
                for k in 0..self.mir.types.len(len) as u32 {
                    self.write_words(element, &format!("{value}[{k}]"), at + k * step, lines)?;
                }
            }
            Ty::Struct(def, args) if self.native_vector(ty).is_none() => {
                let fields: Vec<TypeId> = self.mir.struct_def(def, &args).fields.iter().map(|&(_, t)| t).collect();
                let mut offset = at;
                let mut offsets = Vec::new();
                for field in &fields {
                    offsets.push(offset);
                    offset += self.words(*field)?;
                }
                for (name, member_ty, i) in self.members(ty)? {
                    self.write_words(member_ty, &format!("{value}.{name}"), offsets[i], lines)?;
                }
            }
            Ty::Struct(def, args) => {
                let array = self.mir.struct_def(def, &args).fields[0].1;
                self.write_words(array, value, at, lines)?;
            }
            _ => unreachable!("only these have words"),
        }
        Ok(())
    }

    /// The function reading a `ty` from buffer global `buffer`: `som_load_buf(at)`, made the
    /// first time it's wanted.
    fn load(&mut self, ty: TypeId, buffer: &str) -> Result<String, NoGpu> {
        if let Some(name) = self.loads.get(buffer) {
            return Ok(name.clone());
        }
        self.words(ty)?;
        let native = self.ty(ty)?;
        let value = self.read_words(ty, buffer, 0)?;
        let name = self.unique(format!("som_load_{}", ident(buffer)));
        let prototype = format!("fn {name}(at: u32) -> {native}");
        let definition = format!("{prototype} {{\n    return {value};\n}}\n");
        self.functions.push((prototype, definition));
        self.loads.insert(buffer.to_string(), name.clone());
        Ok(name)
    }

    /// The function writing a `ty` to the kernel's buffer: `som_store_T(at, value)`.
    fn store(&mut self, ty: TypeId) -> Result<String, NoGpu> {
        let native = self.ty(ty)?;
        let mut lines = Vec::new();
        self.write_words(ty, "v", 0, &mut lines)?;
        let name = self.unique(format!("som_store_{}", ident(&native)));
        let prototype = format!("fn {name}(at: u32, v: {native})");
        let definition = format!("{prototype} {{\n{}\n}}\n", lines.join("\n"));
        self.functions.push((prototype, definition));
        Ok(name)
    }

    /// `buffer.get(i)` in WGSL: `buffer`'s item `index`, a `ty`.
    pub(crate) fn buffer_get(&mut self, ty: TypeId, buffer: &str, index: &str) -> Result<String, NoGpu> {
        let load = self.load(ty, buffer)?;
        let words = self.words(ty)?;
        Ok(format!("{load}({index} * {words}u)"))
    }
}

/// What a kernel's parameter is given.
enum Arg {
    Index,
    Uniforms(TypeId),
}

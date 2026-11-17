use crate::Emitter;
use crate::c_types::{c_int, is_promoted};
use som_mir::*;
use std::collections::HashSet;

impl Emitter<'_> {
    pub(crate) fn ty(&self, ty: TypeId) -> String {
        match self.mir.types.get(ty) {
            Ty::Unit => "som_unit".into(),
            Ty::Bool => "bool".into(),
            Ty::Char => "uint32_t".into(),
            Ty::Str => format!("struct {}", self.names.generated[&ty]),
            Ty::CStr => "uint8_t*".into(),
            Ty::Int(int) => c_int(*int).name.into(),
            Ty::Float(FloatTy::F32) => "float".into(),
            Ty::Float(FloatTy::F64) => "double".into(),
            Ty::Pointer | Ty::Fn { .. } => "void*".into(),
            Ty::Ptr(to) => format!("{}*", self.ty(*to)),
            // A slice is both: where its elements start, and how many there are.
            Ty::Ref { to, .. } if self.is_slice(*to) => self.ty(*to),
            Ty::Ref { to, .. } => format!("{}*", self.ty(*to)),
            // A handle is a pointer to a struct only C sees inside.
            Ty::Struct(def, args) if self.mir.struct_def(*def, args).handle => {
                format!("{}*", self.struct_name(*def, args))
            }
            Ty::Struct(def, args) => self.struct_name(*def, args),
            Ty::Optional(_) | Ty::Array { .. } | Ty::Slice(_) | Ty::Range(_) => {
                format!("struct {}", self.names.generated[&ty])
            }
            Ty::Error
            | Ty::Param(_)
            | Ty::Var(_)
            | Ty::Value(_)
            | Ty::Add(..)
            | Ty::Mul(..)
            | Ty::Assoc { .. } => {
                unreachable!("codegen only sees checked, concrete types")
            }
        }
    }

    pub(crate) fn struct_name(&self, def: DefId, args: &[TypeId]) -> String {
        format!("struct {}", self.names.structs[&(def, args.to_vec())])
    }

    fn is_slice(&self, ty: TypeId) -> bool {
        matches!(self.mir.types.get(ty), Ty::Slice(_) | Ty::Str)
    }

    pub(crate) fn is_unit(&self, ty: TypeId) -> bool {
        matches!(self.mir.types.get(ty), Ty::Unit)
    }

    /// Whether `ty` is an integer that C does arithmetic on as `int`.
    pub(crate) fn is_narrow(&self, ty: TypeId) -> bool {
        matches!(self.mir.types.get(ty), Ty::Int(int) if is_promoted(*int))
    }

    /// The C return type of a function. Externs returning `()` return `void`.
    pub(crate) fn ret_ty(&self, func: &Function) -> String {
        if func.link_name.is_some() && self.is_unit(func.ret) {
            "void".into()
        } else {
            self.ty(func.ret)
        }
    }

    /// Struct, optional and array definitions, ordered so that each comes after any it
    /// contains by value.
    pub(crate) fn structs(&mut self) {
        let mir = self.mir;
        let mut types: Vec<TypeId> = mir
            .structs
            .iter()
            .filter_map(|s| mir.types.lookup(&Ty::Struct(s.def, s.args.clone())))
            .collect();
        let mut generated: Vec<TypeId> = self.names.generated.keys().copied().collect();
        generated.sort();
        types.extend(generated);
        for &ty in &types {
            let name = self.ty(ty);
            // A handle's type is a pointer to the struct.
            self.line(format!("{};", name.trim_end_matches('*')));
        }
        let mut done = HashSet::new();
        for &ty in &types {
            self.define(ty, &mut done, &mut Vec::new());
        }
        self.line("");
    }

    /// Defines the struct, optional or array `ty`, after what it contains by value.
    fn define(&mut self, ty: TypeId, done: &mut HashSet<TypeId>, visiting: &mut Vec<TypeId>) {
        // A struct that contains itself by value has no size; the type checker doesn't catch
        // that yet, so emit it anyway and let the C compiler report it.
        if done.contains(&ty) || visiting.contains(&ty) {
            return;
        }
        let contains: Vec<TypeId> = match self.mir.types.get(ty) {
            Ty::Struct(def, args) => (self.mir.struct_def(*def, args).fields.iter())
                .map(|f| f.1)
                .collect(),
            Ty::Optional(inner) | Ty::Array { element: inner, .. } | Ty::Range(inner) => {
                vec![*inner]
            }
            // Its elements are behind a pointer, so they don't have to come first.
            Ty::Slice(_) | Ty::Str => Vec::new(),
            _ => return,
        };
        visiting.push(ty);
        for inner in contains {
            // A slice is a struct of its own, held by value even though it's a reference.
            let inner = match self.mir.types.get(inner) {
                Ty::Ref { to, .. } if self.is_slice(*to) => *to,
                _ => inner,
            };
            self.define(inner, done, visiting);
        }
        visiting.pop();
        done.insert(ty);
        self.definition(ty);
    }

    fn definition(&mut self, ty: TypeId) {
        let name = self.ty(ty);
        let members = match self.mir.types.get(ty).clone() {
            Ty::Struct(def, args) => return self.define_struct(def, &args),
            Ty::Optional(inner) => format!("bool some; {} value;", self.ty(inner)),
            Ty::Slice(element) => format!("{}* ptr; uintptr_t len;", self.ty(element)),
            Ty::Str => "uint8_t* ptr; uintptr_t len;".to_string(),
            Ty::Range(int) => {
                let int = self.ty(int);
                format!("{int} start; {int} end;")
            }
            // C has no empty arrays; one that's never read stands in.
            Ty::Array { element, len } => {
                let len = self.mir.types.len(len).max(1);
                format!("{} items[{len}];", self.ty(element))
            }
            _ => return,
        };
        self.line(format!("{name} {{ {members} }};"));
    }

    fn define_struct(&mut self, def: DefId, args: &[TypeId]) {
        let s = self.mir.struct_def(def, args);
        // Only C knows what's inside: the forward declaration is all there is.
        if s.handle {
            return;
        }
        if s.variants.is_some() {
            return self.define_enum(def, args);
        }
        let names = &self.names.fields[&def];
        let fields: Vec<String> = s
            .fields
            .iter()
            .zip(names)
            .map(|(&(_, ty), name)| format!(" {} {name};", self.ty(ty)))
            .collect();
        // C doesn't allow empty structs.
        let fields = if fields.is_empty() {
            " som_unit unused;".to_string()
        } else {
            fields.concat()
        };
        let name = self.struct_name(def, args);
        self.line(format!("{name} {{{fields} }};"));
    }

    /// `struct Shape { uint32_t tag; float base; union { struct { float _0; } Circle; … } as; };`
    fn define_enum(&mut self, def: DefId, args: &[TypeId]) {
        let s = self.mir.struct_def(def, args);
        let variants = s.variants.as_ref().expect("an enum");
        let names = &self.names.fields[&def];
        let common: String = (s.fields.iter().zip(names).skip(1))
            .filter(|(_, name)| !name.starts_with("as."))
            .map(|(&(_, ty), name)| format!(" {} {name};", self.ty(ty)))
            .collect();
        let mut members = Vec::new();
        for (i, _) in variants.iter().enumerate() {
            let own: Vec<usize> = (s.variant_fields(i).iter().copied())
                .filter(|&index| names[index].starts_with("as."))
                .collect();
            if own.is_empty() {
                continue;
            }
            let mut variant = String::new();
            let mut fields = String::new();
            for index in own {
                let mut parts = names[index].split('.').skip(1);
                variant = parts.next().unwrap_or_default().to_string();
                let field = parts.next().unwrap_or_default();
                fields.push_str(&format!(" {} {field};", self.ty(s.fields[index].1)));
            }
            members.push(format!(" struct {{{fields} }} {variant};"));
        }
        let union = if members.is_empty() {
            String::new()
        } else {
            format!(" union {{{} }} as;", members.concat())
        };
        let name = self.struct_name(def, args);
        self.line(format!("{name} {{ uint32_t tag;{common}{union} }};"));
    }
}

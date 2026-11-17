//! C types as som's:
//!
//! | C                                   | som            |
//! |-------------------------------------|----------------|
//! | `char`, `short`, `int`, `long`, …   | `i8` … `u64`   |
//! | `float`, `double`, `bool`           | `f32`, `f64`, `bool` |
//! | `const char *`                      | `cstr`         |
//! | parameter `const T *`, `T *`        | `&T`, `&mut T` |
//! | `sqlite3 *`, a struct never defined | `Sqlite3`, an `extern struct` handle |
//! | any other pointer                   | `*`            |
//! | `struct Color` (by value)           | `Color`, with the same fields |
//! | `enum …`                            | `i32`          |

use serde_json::Value;
use som_base::diagnostics::{pascal_case, snake_case};
use som_lang::Primitive;
use std::collections::{BTreeMap, HashMap};

use super::identifier;

pub(super) const UNIT: &str = "()";
pub(super) const POINTER: &str = "*";
pub(super) const CSTR: &str = Primitive::CStr.name();

/// C's arithmetic types, each as clang spells it.
const PRIMITIVES: &[(Primitive, &[&str])] = &[
    (Primitive::Bool, &["_Bool", "bool"]),
    (Primitive::I8, &["char", "signed char"]),
    (Primitive::U8, &["unsigned char"]),
    (Primitive::I16, &["short", "short int", "signed short"]),
    (Primitive::U16, &["unsigned short", "unsigned short int"]),
    (Primitive::I32, &["int", "signed", "signed int"]),
    (Primitive::U32, &["unsigned", "unsigned int"]),
    (
        Primitive::I64,
        &[
            "long",
            "long int",
            "long long",
            "long long int",
            "signed long",
        ],
    ),
    (
        Primitive::U64,
        &[
            "unsigned long",
            "unsigned long int",
            "unsigned long long",
            "unsigned long long int",
        ],
    ),
    (Primitive::F32, &["float"]),
    (Primitive::F64, &["double"]),
];

fn primitive(c: &str) -> Option<Primitive> {
    (PRIMITIVES.iter())
        .find(|(_, spellings)| spellings.contains(&c))
        .map(|&(primitive, _)| primitive)
}

/// The type a cast in a `#define` names, `(unsigned int)`, if it's one a constant can
/// have: `int`, `unsigned`, `long`, `unsigned long`, `float` or `double`.
pub(super) fn cast_type(words: &[&str]) -> Option<Primitive> {
    let has = |w: &str| words.contains(&w);
    if words.iter().any(|w| {
        !matches!(
            *w,
            "int" | "unsigned" | "signed" | "long" | "float" | "double" | "const"
        )
    }) {
        return None;
    }
    Some(if has("double") {
        Primitive::F64
    } else if has("float") {
        Primitive::F32
    } else if has("unsigned") && has("long") {
        Primitive::U64
    } else if has("unsigned") {
        Primitive::U32
    } else if has("long") {
        Primitive::I64
    } else {
        Primitive::I32
    })
}

/// A C function's signature in som types.
pub(super) struct Signature {
    pub params: Vec<(String, String)>,
    pub variadic: bool,
    /// `()` for `void`.
    pub returns: String,
}

impl Signature {
    /// `fn name(a: i32, b: &str) -> f32`
    pub fn text(&self, name: &str) -> String {
        let mut params: Vec<String> = self
            .params
            .iter()
            .map(|(n, ty)| format!("{n}: {ty}"))
            .collect();
        if self.variadic {
            params.push("...".into());
        }
        let returns = if self.returns == UNIT {
            String::new()
        } else {
            format!(" -> {}", self.returns)
        };
        format!("fn {name}({}){returns}", params.join(", "))
    }
}

/// Converts a header's C types, importing the structs they use as it goes.
#[derive(Default)]
pub(super) struct Converter {
    /// Struct tag → its field declarations, from the header.
    records: HashMap<String, Vec<Value>>,
    typedefs: HashMap<String, String>,
    /// The structs used so far, converted: som name → fields.
    pub structs: BTreeMap<String, Vec<(String, String)>>,
    /// Each converted struct's C tag, by som name: `MaDevice` → `ma_device`.
    pub tags: HashMap<String, String>,
    /// Structs that can't be converted, and why.
    unsupported: HashMap<String, String>,
    /// Structs the header only declares, used through pointers: som name → C tag.
    /// They become `extern struct`s, whose values are those pointers.
    pub handles: BTreeMap<String, String>,
}

impl Converter {
    pub fn of(decls: &[Value]) -> Converter {
        let mut c = Converter::default();
        for decl in decls {
            match decl["kind"].as_str() {
                Some("RecordDecl") if decl["tagUsed"] == "struct" => {
                    if let (Some(name), Some(fields)) =
                        (decl["name"].as_str(), decl["inner"].as_array())
                    {
                        c.records.insert(name.to_string(), fields.clone());
                    }
                }
                Some("TypedefDecl") => {
                    if let (Some(name), Some(ty)) =
                        (decl["name"].as_str(), decl["type"]["qualType"].as_str())
                    {
                        c.typedefs.insert(name.to_string(), ty.to_string());
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn signature(&mut self, decl: &Value) -> Result<Signature, String> {
        let signature = decl["type"]["qualType"].as_str().unwrap_or_default();
        let returns = self.ty(return_type(signature).ok_or("its return type is too complex")?)?;
        let mut params = Vec::new();
        for (i, param) in decl["inner"].as_array().into_iter().flatten().enumerate() {
            if param["kind"] != "ParmVarDecl" {
                continue;
            }
            let ty = self.param_ty(type_text(&param["type"]))?;
            let name = param["name"]
                .as_str()
                .map_or(format!("arg{i}"), |n| identifier(&snake_case(n)));
            params.push((name, ty));
        }
        Ok(Signature {
            params,
            variadic: decl["variadic"] == true,
            returns,
        })
    }

    /// `pub fn name(a: i32, b: &str) -> f32;`
    pub fn function(&mut self, decl: &Value, name: &str) -> Result<String, String> {
        // Everything a header declares is there to be used: `pub`.
        Ok(format!("pub {};", self.signature(decl)?.text(name)))
    }

    /// Like [`Converter::ty`], except that a pointer to a struct, a number or another
    /// pointer is a reference, `&mut` unless what it points to is `const`. Return values
    /// and fields stay `*`: C can hand back NULL, or memory the caller now owns.
    pub fn param_ty(&mut self, text: &str) -> Result<String, String> {
        let text = without_restrict(text);
        // `int *const`: a pointer that can't be changed, to something that can.
        let text = text.trim();
        let text = (text.strip_suffix("const").map(str::trim_end))
            .filter(|t| t.ends_with('*'))
            .unwrap_or(text);
        let Some(pointee) = text.strip_suffix('*').map(str::trim) else {
            return self.ty(text);
        };
        if self.ty(text)? == CSTR {
            return Ok(CSTR.into());
        }
        let constant = pointee.starts_with("const ") || pointee.ends_with(" const");
        match self.ty(pointee) {
            Ok(to)
                if to != UNIT && to != CSTR && !to.starts_with('&') && !pointee.contains('(') =>
            {
                Ok(format!("&{}{to}", if constant { "" } else { "mut " }))
            }
            // `void *`, an opaque struct, a function pointer, `const char *`.
            _ => self.ty(text),
        }
    }

    /// The som type for a C type, importing any struct it names.
    pub fn ty(&mut self, text: &str) -> Result<String, String> {
        let text = without_restrict(text);
        let text = text.trim();
        let constant = text.starts_with("const ") || text.contains(" const");
        let text = text
            .trim_start_matches("const ")
            .trim_start_matches("volatile ")
            .trim_end_matches(" const")
            .trim();

        if text.contains('[') {
            return Err("it uses an array".into());
        }
        if text.contains('(') {
            return Ok(POINTER.into());
        }
        if let Some(pointee) = text.strip_suffix('*') {
            return Ok(self.pointer(pointee, constant));
        }
        if text == "void" {
            return Ok(UNIT.into());
        }
        if let Some(primitive) = primitive(text) {
            return Ok(primitive.name().into());
        }
        if text == "long double" {
            return Err("it uses `long double`".into());
        }
        if let Some(tag) = text.strip_prefix("struct ") {
            return self.import_struct(tag);
        }
        if text.starts_with("enum ") {
            return Ok(Primitive::I32.name().into());
        }
        if text.starts_with("union ") {
            return Err("it uses a union".into());
        }
        match self.typedefs.get(text).cloned() {
            Some(underlying) if underlying != text => self.ty(&underlying),
            _ => Err(format!("it uses `{text}`, which som can't express yet")),
        }
    }

    fn pointer(&mut self, pointee: &str, constant: bool) -> String {
        let pointee = pointee.trim().trim_start_matches("const ").trim();
        if constant && pointee == "char" {
            return CSTR.into();
        }
        // `sqlite3 *`, where the header never says what's in a `sqlite3`.
        let mut pointee = pointee.to_string();
        while let Some(underlying) = self.typedefs.get(&pointee).filter(|u| **u != pointee) {
            pointee = underlying.trim().to_string();
        }
        if let Some(tag) = pointee.strip_prefix("struct ")
            && !self.records.contains_key(tag)
            && let Some(name) = self.handle(tag)
        {
            return name;
        }
        POINTER.into()
    }

    /// The handle type for pointers to the opaque `struct tag`, or `None` if its som name
    /// is taken by another struct.
    fn handle(&mut self, tag: &str) -> Option<String> {
        let name = pascal_case(tag);
        let taken = self.tags.contains_key(&name)
            || self.handles.get(&name).is_some_and(|other| other != tag);
        if taken {
            return None;
        }
        self.handles.insert(name.clone(), tag.to_string());
        Some(name)
    }

    /// A struct, under its som name: `struct ma_device` is `MaDevice`.
    fn import_struct(&mut self, tag: &str) -> Result<String, String> {
        let name = pascal_case(tag);
        match self.tags.get(&name) {
            Some(other) if other == tag => return Ok(name),
            Some(other) => {
                return Err(format!(
                    "it uses `{tag}`, which in som would be called `{name}`, like `{other}`"
                ));
            }
            None => {}
        }
        if let Some(reason) = self.unsupported.get(tag) {
            return Err(reason.clone());
        }
        if let Some(other) = self.handles.get(&name) {
            return Err(format!(
                "it uses `{tag}`, which in som would be called `{name}`, like `{other}`"
            ));
        }
        let Some(fields) = self.records.get(tag).cloned() else {
            return Err(format!(
                "it uses `struct {tag}`, which the header doesn't define"
            ));
        };
        let mut converted = Vec::new();
        for field in fields.iter().filter(|f| f["kind"] == "FieldDecl") {
            let name = field["name"].as_str().unwrap_or_default().to_string();
            match self.ty(type_text(&field["type"])) {
                Ok(ty) => converted.push((name, ty)),
                Err(reason) => {
                    let reason = format!(
                        "it uses `{tag}`, whose field `{name}` can't be expressed ({reason})"
                    );
                    self.unsupported.insert(tag.to_string(), reason.clone());
                    return Err(reason);
                }
            }
        }
        self.tags.insert(name.clone(), tag.to_string());
        self.structs.insert(name.clone(), converted);
        Ok(name)
    }
}

/// A type with its typedefs resolved, as clang writes it.
fn type_text(ty: &Value) -> &str {
    ty["desugaredQualType"]
        .as_str()
        .or(ty["qualType"].as_str())
        .unwrap_or_default()
}

/// `text` without `restrict`, a promise to the C compiler that pointers don't overlap,
/// which som has no use for: `char *restrict` is `char *`.
fn without_restrict(text: &str) -> String {
    text.replace("__restrict", "restrict")
        .split_inclusive(['*', ' '])
        .filter(|part| part.trim() != "restrict")
        .collect::<String>()
        .replace("*restrict", "*")
        .trim()
        .to_string()
}

/// The return type in a function type like `const char *(int, float)`: everything
/// before the parenthesised parameter list at the end.
fn return_type(signature: &str) -> Option<&str> {
    let signature = signature.trim();
    if !signature.ends_with(')') {
        return None;
    }
    let mut depth = 0;
    for (i, c) in signature.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' => {
                depth -= 1;
                if depth == 0 {
                    let returns = signature[..i].trim();
                    // `void (*(int))(int)`, a function returning a function pointer.
                    return (!returns.ends_with(')')).then_some(returns);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_types() {
        assert_eq!(return_type("void (int, int, const char *)"), Some("void"));
        assert_eq!(return_type("const char *(void)"), Some("const char *"));
        assert_eq!(return_type("Color (int, int (*)(int))"), Some("Color"));
    }

    #[test]
    fn types() {
        let mut c = Converter::default();
        c.typedefs.insert("size_t".into(), "unsigned long".into());
        assert_eq!(c.ty("const char *").unwrap(), "cstr");
        assert_eq!(c.ty("char *").unwrap(), "*");
        assert_eq!(c.ty("size_t").unwrap(), "u64");
        assert_eq!(c.ty("void (*)(int)").unwrap(), "*");
        assert_eq!(c.ty("enum Mode").unwrap(), "i32");
        assert!(c.ty("float [4]").is_err());
    }

    #[test]
    fn pointer_parameters_are_references() {
        let mut c = Converter::default();
        assert_eq!(c.param_ty("const float *").unwrap(), "&f32");
        assert_eq!(c.param_ty("int *").unwrap(), "&mut i32");
        assert_eq!(c.param_ty("int *const").unwrap(), "&mut i32");
        assert_eq!(c.param_ty("struct opaque **").unwrap(), "&mut Opaque");
        assert_eq!(c.param_ty("const char *").unwrap(), "cstr");
        assert_eq!(c.param_ty("const char *restrict").unwrap(), "cstr");
        assert_eq!(
            c.param_ty("FILE *restrict").unwrap(),
            c.param_ty("FILE *").unwrap()
        );
        assert_eq!(c.param_ty("void *__restrict").unwrap(), "*");
        assert_eq!(c.param_ty("const char **").unwrap(), "*");
        assert_eq!(c.param_ty("void *").unwrap(), "*");
        assert_eq!(c.param_ty("struct opaque *").unwrap(), "Opaque");
        assert_eq!(c.param_ty("void (*)(int)").unwrap(), "*");
        assert_eq!(c.param_ty("int").unwrap(), "i32");
    }
}

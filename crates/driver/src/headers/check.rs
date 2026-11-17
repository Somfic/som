//! Hand-written `extern` declarations, compared with the header they could have been
//! imported from.

use serde_json::Value;
use som_ast::{Ast, Extern, Func, Item};
use som_base::diagnostics::snake_case;
use som_base::message::{self as m, Message};
use som_base::msg;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use super::ctype::{CSTR, Converter, POINTER, Signature, UNIT};
use super::{LIBC, clang, find_header, identifier, module_name, top_level};
use crate::libraries::{LibraryError, LibraryWarning, SameAsHeader};

/// Struct name → its fields' names and som types.
type Structs = BTreeMap<String, Vec<(String, String)>>;

/// What a header declares, in som terms.
struct Declarations {
    /// Every function som can express, by name.
    functions: HashMap<String, Signature>,
    /// The structs those functions use.
    structs: Structs,
}

impl Declarations {
    fn of(json: &Value) -> Declarations {
        let decls = top_level(json);
        let mut c = Converter::of(decls);
        let functions = decls
            .iter()
            .filter(|d| d["kind"] == "FunctionDecl")
            .filter_map(|d| Some((d["name"].as_str()?.to_string(), c.signature(d).ok()?)))
            .collect();
        Declarations {
            functions,
            structs: c.structs,
        }
    }
}

/// Compares hand-written `extern "library" { … }` declarations with the library's own
/// header, `library.h`, if it can be found. Blocks that import a header are skipped:
/// their declarations come from it. Declarations that disagree with the header are
/// errors; ones that are exactly the header's, which `extern "library";` would import,
/// are warnings.
pub fn check_declarations(
    ast: &Ast,
    library_dirs: &[PathBuf],
) -> (Vec<LibraryError>, Vec<LibraryWarning>) {
    let root = ast.modules[0].path.clone();
    let mut headers: HashMap<String, Option<Declarations>> = HashMap::new();
    let mut errors = check_libc_declarations(ast);
    let mut warnings = Vec::new();
    let mut warned_structs = HashSet::new();

    for item in ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let (Some(library), None) = (&block.library, &block.header) else {
            continue;
        };
        let header = format!("{library}.h");
        let declared = headers.entry(library.clone()).or_insert_with(|| {
            let path = find_header(&header, &root, library_dirs).ok()?;
            let json = clang::syntax_tree(&path, &root, library_dirs).ok()?;
            Some(Declarations::of(&json))
        });
        let Some(declared) = declared else { continue };
        let module = module_name(Some(library), &header);

        let mut same = Vec::new();
        let mut struct_names = Vec::new();
        for &id in &block.funcs {
            let func = &ast.funcs[id];
            // Compared by the C symbol it links to: `init_window` is `InitWindow` there.
            let symbol = func.link_name.as_deref().unwrap_or(&func.name.name);
            let Some(expected) = declared.functions.get(symbol) else {
                continue;
            };
            if let Some(problem) = mismatch(ast, func, expected, &declared.structs) {
                errors.push(LibraryError::HeaderMismatch {
                    function: func.name.name.to_string(),
                    header: header.clone(),
                    declared_there: expected.text(&func.name.name),
                    problem,
                    span: func.name.span.clone(),
                });
            } else if let Some(structs) = identical(ast, func, expected, &declared.structs) {
                same.push(SameAsHeader {
                    name: func.name.name.to_string(),
                    span: func.name.span.clone(),
                    declaration: ast.funcs.span(id).clone(),
                    imported: format!("{module}::{}", identifier(&snake_case(symbol))),
                });
                struct_names.extend(structs);
            }
        }
        if same.is_empty() {
            continue;
        }
        warnings.extend(structs_in_header(
            ast,
            &struct_names,
            library,
            &header,
            &module,
            &mut warned_structs,
        ));
        warnings.push(declared_in_header(block, library, header, same));
    }
    warnings.sort_by_key(|w| {
        let span = w.span();
        (span.source.identifier().to_string(), span.start_offset)
    });
    (errors, warnings)
}

fn declared_in_header(
    block: &Extern,
    library: &str,
    header: String,
    same: Vec<SameAsHeader>,
) -> LibraryWarning {
    LibraryWarning::DeclaredInHeader {
        library: library.to_string(),
        header,
        span: block.library_span.clone().unwrap_or(same[0].span.clone()),
        block: block.span.clone(),
        all: same.len() == block.funcs.len(),
        functions: same,
    }
}

/// The structs named in `names` that are written by hand to match the header's, each
/// warned about once.
fn structs_in_header(
    ast: &Ast,
    names: &[String],
    library: &str,
    header: &str,
    module: &str,
    warned: &mut HashSet<som_ast::Id<som_ast::Struct>>,
) -> Vec<LibraryWarning> {
    let mut warnings = Vec::new();
    for name in names {
        for (id, s) in ast.structs.iter() {
            if *s.name.name == **name
                && !s.copy
                && s.variants.is_none()
                && s.generics.is_empty()
                && warned.insert(id)
            {
                warnings.push(LibraryWarning::StructInHeader {
                    name: name.clone(),
                    library: library.to_string(),
                    header: header.to_string(),
                    imported: format!("{module}::{name}"),
                    span: s.name.span.clone(),
                    declaration: ast.structs.span(id).clone(),
                });
            }
        }
    }
    warnings
}

/// Whether a declaration that agrees with the header's is exactly it, so importing the
/// header gives the same function. Agreeing is looser: any pointer for any other, a
/// struct for another with the same fields. A declaration that only agrees says
/// something the header doesn't, like `&Color` for a `Color *`, and is worth writing.
/// Returns the header's structs it uses, by name.
fn identical(
    ast: &Ast,
    func: &Func,
    expected: &Signature,
    structs: &Structs,
) -> Option<Vec<String>> {
    let mut used = Vec::new();
    let mut same = |written: String, header: &str| {
        if written == header {
            if structs.contains_key(header) {
                used.push(header.to_string());
            }
            return true;
        }
        false
    };
    let all = func.variadic == expected.variadic
        && func.params.len() == expected.params.len()
        && same(return_text(ast, func), &expected.returns)
        && (func.params.iter())
            .zip(&expected.params)
            .all(|(p, (_, ty))| same(param_text(ast, p), ty));
    all.then_some(used)
}

/// Compares hand-written `extern { … }` declarations, which link to the C library, with
/// what its headers say, as imported into module `c`. Only how the function is called is
/// compared: a variadic function called as if it weren't (or the other way round) gets
/// its arguments in the wrong places, without any error. Types are left alone, as C
/// converts between its integers and pointers where they're passed.
fn check_libc_declarations(ast: &Ast) -> Vec<LibraryError> {
    let mut libc: HashMap<&str, &Func> = HashMap::new();
    for item in ast.modules.iter().flat_map(|m| &m.items) {
        if let Item::Extern(block) = item
            && block.library.as_deref() == Some(LIBC)
            && block.header.is_some()
        {
            for &id in &block.funcs {
                let func = &ast.funcs[id];
                libc.insert(func.link_name.as_deref().unwrap_or(&func.name.name), func);
            }
        }
    }
    let mut errors = Vec::new();
    for item in ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        if block.library.is_some() || block.header.is_some() {
            continue;
        }
        for &id in &block.funcs {
            let func = &ast.funcs[id];
            let symbol = func.link_name.as_deref().unwrap_or(&func.name.name);
            let Some(expected) = libc.get(symbol) else {
                continue;
            };
            let n = expected.params.len();
            let problem = match variadic_mismatch(expected.variadic, func.variadic) {
                Some(problem) => problem,
                None if func.params.len() != n => parameter_count(n, func.params.len()),
                None => continue,
            };
            errors.push(LibraryError::HeaderMismatch {
                function: func.name.name.to_string(),
                header: "libc".into(),
                declared_there: func_text(ast, expected),
                problem,
                span: func.name.span.clone(),
            });
        }
    }
    errors
}

/// `fn name(a: i32, ...) -> i32`, of a declaration.
fn func_text(ast: &Ast, func: &Func) -> String {
    let mut params: Vec<String> = (func.params.iter())
        .map(|p| format!("{}: {}", p.name.name, param_text(ast, p)))
        .collect();
    if func.variadic {
        params.push("...".into());
    }
    let returns = func
        .returns
        .map_or(String::new(), |t| format!(" -> {}", ast.type_text(t)));
    format!("fn {}({}){returns}", func.name.name, params.join(", "))
}

fn param_text(ast: &Ast, param: &som_ast::Param) -> String {
    param.ty.map_or("_".into(), |t| ast.type_text(t))
}

fn return_text(ast: &Ast, func: &Func) -> String {
    func.returns.map_or(UNIT.into(), |t| ast.type_text(t))
}

fn parameter_count(expected: usize, written: usize) -> Message {
    msg![format!(
        "takes {expected} parameter{} there, not {written}",
        if expected == 1 { "" } else { "s" },
    )]
}

fn variadic_mismatch(expected: bool, written: bool) -> Option<Message> {
    match (expected, written) {
        (true, false) => Some(msg!["takes extra arguments (", m::code("..."), ") there"]),
        (false, true) => Some(msg!["doesn't take extra arguments there"]),
        _ => None,
    }
}

/// How a declaration differs from the header's, if it does.
fn mismatch(ast: &Ast, func: &Func, expected: &Signature, structs: &Structs) -> Option<Message> {
    let compatible = |written: &str, header: &str| compatible(ast, structs, written, header);
    let types: Vec<String> = func.params.iter().map(|p| param_text(ast, p)).collect();
    if types.len() != expected.params.len() {
        return Some(parameter_count(expected.params.len(), types.len()));
    }
    for (written, (name, ty)) in types.iter().zip(&expected.params) {
        if !compatible(written, ty) {
            return Some(msg![
                "parameter ",
                m::var(name),
                " is ",
                m::ty(ty),
                " there, not ",
                m::ty(written)
            ]);
        }
    }
    if let Some(problem) = variadic_mismatch(expected.variadic, func.variadic) {
        return Some(problem);
    }
    let returns = return_text(ast, func);
    if !compatible(&returns, &expected.returns) {
        return Some(if expected.returns == UNIT {
            msg!["returns nothing there"]
        } else {
            msg![
                "returns ",
                m::ty(&expected.returns),
                " there, not ",
                m::ty(&returns)
            ]
        });
    }
    None
}

/// Whether a type written in som agrees with the header's. All pointers are alike to C,
/// so `*`, `cstr` and references are interchangeable, and a struct agrees with another struct whose
/// fields have the same types in the same order, whatever either is called.
fn compatible(ast: &Ast, structs: &Structs, written: &str, header: &str) -> bool {
    let pointer = |t: &str| t == POINTER || t == CSTR || t.starts_with('&');
    if written == header || (pointer(written) && pointer(header)) {
        return true;
    }
    let Some(theirs) = structs.get(header) else {
        return false;
    };
    // `geo::Shade` is the struct `Shade`.
    let name = written.rsplit("::").next().unwrap_or(written);
    ast.structs.iter().any(|(_, ours)| {
        *ours.name.name == *name
            && ours.fields.len() == theirs.len()
            && (ours.fields.iter())
                .zip(theirs)
                .all(|(field, (_, ty))| compatible(ast, structs, &ast.type_text(field.ty), ty))
    })
}

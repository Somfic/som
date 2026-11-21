//! Shaders: a `@vertex` or `@fragment` function and what it calls, from MIR to GLSL 3.30, the
//! language OpenGL 3.3, and so raylib, takes. A shader is an ordinary function otherwise: it
//! runs on the CPU too, so it can be tested there.
//!
//! | MIR                             | GLSL                                                  |
//! |---------------------------------|-------------------------------------------------------|
//! | struct, enum, optional, range   | struct (an enum's tag is a `uint`)                   |
//! | `[T; N]`                        | `T[N]`                                                |
//! | `usize`                         | `uint`: the GPU's integers are 32 bits                |
//! | `&T` parameter                  | `T`: a copy, as nothing changes it while the call runs |
//! | `&mut T` parameter              | `inout T`: copied in and back out, which the borrow checker makes the same as through a pointer |
//! | local reference                 | the place it refers to, written out where it's used   |
//! | one block                       | its statements                                        |
//! | `if`, `while`, `loop`, `match`  | `if` and `for (;;)` with `break`/`continue` (see [`structure`]) |
//! | any other jumps                 | a `switch` on the block, in a loop                    |
//! | `som::gl::sin`, …               | GLSL's own `sin`, …: std's math is `@if GPU` on them  |
//!
//! What the GPU has no way to do (text, pointers, memory, 64-bit numbers, calling C, failing,
//! calling itself) is an error, said where the shader reaches it and through which calls.

mod body;
mod entry;
mod error;
mod names;
mod structure;
mod types;

pub use error::ShaderError;

use error::Problem;
use som_base::Span;
use som_base::message::code;
use som_base::msg;
use som_lang::Stage;
use som_mir::*;
use std::collections::{HashMap, HashSet};

/// `name` without its modules: `som::data::Vector<4, f32>` is `Vector<4, f32>`, and
/// `som::io::println<som::data::string>` is `println<string>`.
pub(crate) fn short(name: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    for c in name.chars().chain(std::iter::once(' ')) {
        if c.is_alphanumeric() || c == '_' || c == ':' {
            word.push(c);
            continue;
        }
        // A path: its last segment, and the type before it for a method, `Vector::x`.
        let segments: Vec<&str> = word.split("::").collect();
        let keep = match segments.as_slice() {
            [.., owner, _] if owner.starts_with(char::is_uppercase) => 2,
            _ => 1,
        };
        out.push_str(&segments[segments.len().saturating_sub(keep)..].join("::"));
        word.clear();
        out.push(c);
    }
    out.pop();
    out
}

/// A function as messages name it: without its modules or type arguments.
pub fn short_fn(name: &str) -> String {
    let short = short(name);
    match short.find('<') {
        Some(at) => short[..at].to_string(),
        None => short,
    }
}

/// The first line of function `span`, before its body: where to point at the function as a
/// whole.
pub(crate) fn head(span: &Span) -> Span {
    let content = span.source.content();
    let text = &content[span.start_offset..span.start_offset + span.length];
    let end = text.find(['{', '\n']).unwrap_or(text.len());
    let end = text[..end].trim_end().len().max(1).min(text.len());
    Span::from_range(span.start_offset..span.start_offset + end, span.source.clone())
}

/// The GLSL 3.30 source of shader `entry`: what a graphics library loads.
pub fn glsl(mir: &Mir, entry: FuncId) -> Result<String, Vec<ShaderError>> {
    let mut t = Translator {
        mir,
        entry,
        structs: Vec::new(),
        struct_names: HashMap::new(),
        func_names: HashMap::new(),
        used: HashSet::new(),
        uniforms: None,
        reached: HashMap::new(),
        queue: Vec::new(),
        calls: Vec::new(),
        functions: Vec::new(),
        errors: Vec::new(),
    };
    let source = t.run();
    let mut errors = t.errors;
    // The same problem reached from two places, said once.
    let mut seen = Vec::new();
    errors.retain(|e| {
        let key = (e.span.clone(), e.message.clone());
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    match errors.is_empty() {
        true => Ok(source),
        false => Err(errors),
    }
}

pub(crate) struct Translator<'a> {
    pub mir: &'a Mir,
    entry: FuncId,
    /// GLSL struct declarations, each after those it's made of.
    pub structs: Vec<String>,
    pub struct_names: HashMap<TypeId, String>,
    func_names: HashMap<FuncId, String>,
    /// Names given to structs and functions.
    used: HashSet<String>,
    /// The shader's struct of uniforms: its textures are GLSL's uniforms themselves, not
    /// members, since GLSL can't make a struct holding one.
    pub uniforms: Option<TypeId>,
    /// How each function the shader reaches is first reached: by which function, where.
    reached: HashMap<FuncId, (FuncId, Span)>,
    queue: Vec<FuncId>,
    /// Every call between the shader's functions, for finding any that call themselves.
    calls: Vec<(FuncId, FuncId, Span)>,
    /// Each function's prototype and definition.
    functions: Vec<(String, String)>,
    errors: Vec<ShaderError>,
}

impl Translator<'_> {
    fn run(&mut self) -> String {
        let main = match self.mir.stages.get(&self.entry) {
            None => {
                let func = &self.mir.funcs[self.entry];
                self.fail(
                    self.entry,
                    Problem {
                        message: msg![code(short_fn(&func.name)), " isn't a shader"],
                        span: head(&func.span),
                        label: "defined here".into(),
                        hint: Some(msg![
                            "put ",
                            code(Stage::Fragment.name()),
                            " or ",
                            code(Stage::Vertex.name()),
                            " on the line above it"
                        ]),
                    },
                );
                return String::new();
            }
            Some(Stage::Vertex) => {
                let func = &self.mir.funcs[self.entry];
                self.fail(
                    self.entry,
                    Problem {
                        message: msg![
                            code(Stage::Vertex.name()),
                            " shaders can't be compiled for the GPU yet"
                        ],
                        span: head(&func.span),
                        label: "a vertex shader".into(),
                        hint: Some(msg![
                            "only ",
                            code(Stage::Fragment.name()),
                            " shaders can, for now; raylib's own vertex shader goes with them"
                        ]),
                    },
                );
                return String::new();
            }
            Some(Stage::Fragment) => self.fragment(),
        };
        // What's inside can't be said to be wrong for a shader that's wrong outside.
        if !self.errors.is_empty() {
            return String::new();
        }
        self.queue.push(self.entry);
        while let Some(func) = self.queue.pop() {
            self.function(func);
        }
        self.recursion();

        let mut out = String::from("#version 330\n\n");
        out.push_str(&main.globals);
        out.push('\n');
        for s in &self.structs {
            out.push_str(s);
            out.push('\n');
        }
        for (prototype, _) in &self.functions {
            out.push_str(&format!("{prototype};\n"));
        }
        out.push('\n');
        for (_, definition) in &self.functions {
            out.push_str(definition);
            out.push('\n');
        }
        out.push_str(&main.main);
        out
    }

    /// `base`, or with a number after it if that's taken.
    pub(crate) fn unique(&mut self, base: String) -> String {
        let mut name = base.clone();
        let mut n = 2;
        while names::is_reserved(&name) || !self.used.insert(name.clone()) {
            name = format!("{base}_{n}");
            n += 1;
        }
        name
    }

    pub(crate) fn func_name(&mut self, func: FuncId) -> String {
        if let Some(name) = self.func_names.get(&func) {
            return name.clone();
        }
        // GLSL's own functions can't be declared again: `sin`, `mix`, `length`.
        let name = self.unique(format!("f_{}", names::ident(&self.mir.funcs[func].name)));
        self.func_names.insert(func, name.clone());
        name
    }

    /// `callee` is called by `caller` at `span`: it's translated too, if it isn't yet.
    pub(crate) fn reach(&mut self, caller: FuncId, callee: FuncId, span: &Span) {
        self.calls.push((caller, callee, span.clone()));
        if callee != self.entry && !self.reached.contains_key(&callee) {
            self.reached.insert(callee, (caller, span.clone()));
            self.queue.push(callee);
        }
    }

    /// The calls from the shader to `func`, from the shader in.
    fn calls_to(&self, mut func: FuncId) -> Vec<(String, Span)> {
        let mut calls = Vec::new();
        while let Some((caller, span)) = self.reached.get(&func) {
            calls.push((short_fn(&self.mir.funcs[func].name), span.clone()));
            func = *caller;
        }
        calls.reverse();
        calls
    }

    /// `problem`, in `func`.
    pub(crate) fn fail(&mut self, func: FuncId, problem: Problem) {
        self.errors.push(ShaderError {
            shader: short_fn(&self.mir.funcs[self.entry].name),
            message: problem.message,
            span: problem.span,
            label: problem.label,
            hint: problem.hint,
            calls: self.calls_to(func),
        });
    }

    /// GLSL can't call a function from itself, even through others.
    fn recursion(&mut self) {
        let mut callees: HashMap<FuncId, Vec<(FuncId, Span)>> = HashMap::new();
        for (caller, callee, span) in &self.calls {
            callees
                .entry(*caller)
                .or_default()
                .push((*callee, span.clone()));
        }
        // Depth first, from the shader: a call to a function still being walked is a cycle.
        let mut walking = vec![self.entry];
        let mut done = HashSet::new();
        let mut found = None;
        fn walk(
            func: FuncId,
            callees: &HashMap<FuncId, Vec<(FuncId, Span)>>,
            walking: &mut Vec<FuncId>,
            done: &mut HashSet<FuncId>,
            found: &mut Option<(FuncId, FuncId, Span)>,
        ) {
            for (callee, span) in callees.get(&func).into_iter().flatten() {
                if found.is_some() {
                    return;
                }
                if walking.contains(callee) {
                    *found = Some((func, *callee, span.clone()));
                    return;
                }
                if done.insert(*callee) {
                    walking.push(*callee);
                    walk(*callee, callees, walking, done, found);
                    walking.pop();
                }
            }
        }
        walk(self.entry, &callees, &mut walking, &mut done, &mut found);
        if let Some((caller, callee, span)) = found {
            let name = short_fn(&self.mir.funcs[callee].name);
            self.fail(
                caller,
                Problem {
                    message: msg!["a shader can't call ", code(&name), " from inside itself"],
                    span,
                    label: msg![code(format!("{name}(…)")), " is called again here"],
                    hint: Some("the GPU has no stack for recursion: write it as a loop".into()),
                },
            );
        }
    }
}

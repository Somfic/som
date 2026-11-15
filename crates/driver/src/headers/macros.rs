//! A header's `#define`s that stand for a value, as som constants:
//!
//! | C                                                  | som                                    |
//! |----------------------------------------------------|----------------------------------------|
//! | `#define MAX_POINTS 10`                            | `pub const MAX_POINTS: i32 = 10;`      |
//! | `#define PI 3.14159265358979323846f`               | `pub const PI: f32 = 3.1415927;`       |
//! | `#define DEG2RAD (PI/180.0f)`                      | `pub const DEG2RAD: f32 = 0.017453292;`|
//! | `#define VERSION "5.5"`                            | `pub const VERSION: cstr = "5.5";`     |
//! | `#define RED CLITERAL(Color){ 230, 41, 55, 255 }`  | `Color::RED`, in `impl Color { … }`    |
//!
//! The definitions are read from the preprocessor (`-E -dD`), which says which file each is
//! in. Each value is worked out here, the way C would, and written as a literal; a
//! definition that isn't a value som can write (a function-like macro, a type, a call, an
//! `extern`) is left out.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

/// A struct a compound literal can make: its som name, and its fields' som names and types.
pub(super) struct Record {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

/// One `#define NAME body`, and where it's written.
struct Define {
    name: String,
    body: String,
    file: String,
    line: usize,
}

/// The `pub const`s for the definitions in `preprocessed` (the preprocessor's `-E -dD`
/// output) that `wanted` says belong to the module, by their file and name; and their
/// origins, as lines of `name\tfile\tline`. `record` finds the struct a C type name is, for
/// compound literals. Names in `taken` are left out.
pub(super) fn constants(
    preprocessed: &str,
    wanted: &dyn Fn(&str, &str) -> bool,
    record: &dyn Fn(&str) -> Option<Record>,
    taken: &HashSet<String>,
) -> (String, String) {
    let defines = defines(preprocessed);
    let by_name: HashMap<&str, &Define> = defines.iter().map(|d| (d.name.as_str(), d)).collect();
    let mut eval = Evaluator {
        defines: &by_name,
        record,
        values: HashMap::new(),
        evaluating: HashSet::new(),
    };
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    let mut out = String::new();
    // A struct's go in an impl block for it: `Color::RED`.
    let mut of_structs: BTreeMap<String, String> = BTreeMap::new();
    let mut origins = String::new();
    for define in &defines {
        if !wanted(&define.file, &define.name)
            || taken.contains(&define.name)
            || super::SOM_KEYWORDS.contains(&define.name.as_str())
        {
            continue;
        }
        let Some(value) = eval.value(&define.name) else {
            continue;
        };
        let Some(text) = value.declaration(&define.name) else {
            continue;
        };
        let (out, indent) = match &value {
            Val::Struct(strukt, _) => (of_structs.entry(strukt.clone()).or_default(), "    "),
            _ => (&mut out, ""),
        };
        // The comment after it on its line: `// Light Gray`.
        let line = (texts.entry(define.file.clone()))
            .or_insert_with(|| std::fs::read_to_string(&define.file).ok())
            .as_deref()
            .and_then(|text| text.lines().nth(define.line.checked_sub(1)?))
            .filter(|line| line.contains("define") && line.contains(define.name.as_str()));
        if let Some(line) = line {
            let after = &line[line.find(define.name.as_str()).unwrap_or(0)..];
            let comment = after.find("//").or_else(|| after.find("/*"));
            for doc in comment.map_or(Vec::new(), |at| super::comment_lines(&after[at..])) {
                let _ = writeln!(out, "{indent}/// {doc}");
            }
            let _ = writeln!(origins, "{}\t{}\t{}", define.name, define.file, define.line);
        }
        let _ = writeln!(out, "{indent}{text}");
    }
    if !out.is_empty() {
        out.push('\n');
    }
    for (strukt, consts) in of_structs {
        let _ = writeln!(out, "impl {strukt} {{\n{consts}}}\n");
    }
    (out, origins)
}

/// The object-like definitions in the preprocessor's output, in order, each with the file
/// and line it's written on. A name defined again is its last definition; `#undef` removes it.
fn defines(preprocessed: &str) -> Vec<Define> {
    let mut found: Vec<Define> = Vec::new();
    let mut file = String::new();
    let mut line = 0;
    for text in preprocessed.lines() {
        // `# 12 "raylib.h" 2`: the next line is line 12 of `raylib.h`.
        if let Some(marker) = text.strip_prefix("# ")
            && let Some((number, rest)) = marker.split_once(' ')
            && let Ok(number) = number.parse::<usize>()
            && let Some(name) = rest.strip_prefix('"')
            && let Some(end) = name.find('"')
        {
            file = name[..end].to_string();
            line = number;
            continue;
        }
        let here = line;
        line += 1;
        if let Some(name) = text.strip_prefix("#undef ") {
            found.retain(|d| d.name != name.trim());
            continue;
        }
        let Some(rest) = text.strip_prefix("#define ") else {
            continue;
        };
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (name, body) = rest.split_at(end);
        // `#define MAX(a, b) …` takes arguments: not a value.
        if name.is_empty() || body.starts_with('(') {
            continue;
        }
        found.retain(|d| d.name != name);
        found.push(Define {
            name: name.to_string(),
            body: body.trim().to_string(),
            file: file.clone(),
            line: here,
        });
    }
    found
}

/// C's integer types a definition's value can have: `int`, `unsigned`, `long`, `unsigned
/// long`, from narrowest to widest as C ranks them when they meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Int {
    I32,
    U32,
    I64,
    U64,
}

impl Int {
    fn bits(self) -> u32 {
        match self {
            Int::I32 | Int::U32 => 32,
            Int::I64 | Int::U64 => 64,
        }
    }

    fn signed(self) -> bool {
        matches!(self, Int::I32 | Int::I64)
    }

    fn som(self) -> &'static str {
        match self {
            Int::I32 => "i32",
            Int::U32 => "u32",
            Int::I64 => "i64",
            Int::U64 => "u64",
        }
    }

    /// `n` as a value of this type: an unsigned one wraps around, as C's do; a signed one
    /// that doesn't fit is no value.
    fn fit(self, n: i128) -> Option<i128> {
        let bits = self.bits();
        match self.signed() {
            true => {
                let max = (1i128 << (bits - 1)) - 1;
                (-max - 1 <= n && n <= max).then_some(n)
            }
            false => Some(n.rem_euclid(1i128 << bits)),
        }
    }
}

/// What a definition stands for.
#[derive(Clone, Debug)]
enum Val {
    Int(i128, Int),
    /// With whether it's a `double`, or a `float`.
    Float(f64, bool),
    Str(String),
    /// A struct's som name, and its fields' values as som writes them.
    Struct(String, Vec<(String, String)>),
}

impl Val {
    /// `pub const NAME: i32 = 10;`
    fn declaration(&self, name: &str) -> Option<String> {
        Some(match self {
            Val::Int(n, int) => format!("pub const {name}: {} = {n};", int.som()),
            Val::Float(x, double) => {
                let ty = if *double { "f64" } else { "f32" };
                format!("pub const {name}: {ty} = {};", float_text(*x, *double)?)
            }
            Val::Str(text) => format!("pub const {name}: cstr = \"{}\";", som_escaped(text)),
            Val::Struct(strukt, fields) => {
                let fields: Vec<String> = (fields.iter())
                    .map(|(field, value)| format!("{field}: {value}"))
                    .collect();
                format!("pub const {name} = {strukt} {{ {} }};", fields.join(", "))
            }
        })
    }

    /// As C converts it to integer type `int`.
    fn to_int(&self, int: Int) -> Option<Val> {
        let n = match *self {
            Val::Int(n, _) => n,
            Val::Float(x, _) if x.is_finite() => x.trunc() as i128,
            _ => return None,
        };
        Some(Val::Int(int.fit(n)?, int))
    }

    /// As C converts it to `double` (or `float`).
    fn to_float(&self, double: bool) -> Option<Val> {
        let x = match *self {
            Val::Int(n, _) => n as f64,
            Val::Float(x, _) => x,
            _ => return None,
        };
        Some(Val::Float(round(x, double), double))
    }
}

/// `x` as a `double` holds it, or as a `float` does.
fn round(x: f64, double: bool) -> f64 {
    if double { x } else { x as f32 as f64 }
}

/// A float literal som reads back as `x`: digits, a point, and digits. `None` for what
/// isn't a number.
fn float_text(x: f64, double: bool) -> Option<String> {
    if !x.is_finite() {
        return None;
    }
    let mut text = match double {
        true => format!("{x}"),
        false => format!("{}", x as f32),
    };
    if !text.contains('.') {
        text.push_str(".0");
    }
    Some(text)
}

/// A piece of a definition's body.
#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(String),
    Str(String),
    Ident(String),
    Punct(&'static str),
}

const PUNCTS: &[&str] = &[
    "<<", ">>", "(", ")", "{", "}", ",", "+", "-", "*", "/", "%", "&", "|", "^", "~",
];

/// The pieces of `body`, or `None` if it has something a value can't.
fn lex(body: &str) -> Option<Vec<Tok>> {
    let chars: Vec<char> = body.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let start = i;
            let hex = c == '0' && matches!(chars.get(i + 1), Some('x' | 'X'));
            while i < chars.len() {
                let c = chars[i];
                let exponent = !hex
                    && matches!(c, '+' | '-')
                    && matches!(chars[i - 1], 'e' | 'E');
                if !(c.is_ascii_alphanumeric() || c == '.' || exponent) {
                    break;
                }
                i += 1;
            }
            tokens.push(Tok::Num(chars[start..i].iter().collect()));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(Tok::Ident(chars[start..i].iter().collect()));
        } else if c == '"' {
            // The text it stands for, C's escapes worked out: written again as som's.
            let mut text = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] != '\\' {
                    text.push(chars[i]);
                    i += 1;
                    continue;
                }
                let (byte, len) = c_escape(&chars[i + 1..])?;
                // Past `\x7f` it's a byte of something, not a character: left out.
                if byte > 0x7f {
                    return None;
                }
                text.push(char::from_u32(byte)?);
                i += 1 + len;
            }
            if i >= chars.len() {
                return None;
            }
            i += 1;
            // `"a" "b"` is `"ab"`.
            match tokens.last_mut() {
                Some(Tok::Str(before)) => before.push_str(&text),
                _ => tokens.push(Tok::Str(text)),
            }
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let punct = PUNCTS.iter().find(|p| rest.starts_with(**p))?;
            i += punct.len();
            tokens.push(Tok::Punct(punct));
        }
    }
    Some(tokens)
}

/// The byte C escape `\…` stands for, from what follows its `\`, and how many characters
/// that is: `n` is a newline, `033` and `x1b` are 27.
fn c_escape(after: &[char]) -> Option<(u32, usize)> {
    let simple = |c: char| -> Option<u32> {
        Some(match c {
            'n' => 10,
            't' => 9,
            'r' => 13,
            'a' => 7,
            'b' => 8,
            'f' => 12,
            'v' => 11,
            'e' => 27,
            '\\' | '"' | '\'' | '?' => c as u32,
            _ => return None,
        })
    };
    let first = *after.first()?;
    if let Some(value) = simple(first) {
        return Some((value, 1));
    }
    let (radix, digits, skip): (u32, Vec<char>, usize) = match first {
        'x' => (16, after[1..].iter().copied().take_while(char::is_ascii_hexdigit).collect(), 1),
        '0'..='7' => (8, after.iter().copied().take(3).take_while(|c| ('0'..='7').contains(c)).collect(), 0),
        _ => return None,
    };
    if digits.is_empty() {
        return None;
    }
    let value = u32::from_str_radix(&digits.iter().collect::<String>(), radix).ok()?;
    Some((value, skip + digits.len()))
}

/// `text` as som writes it between quotes: `"`, `\`, `{` and `}` and control characters
/// escaped.
fn som_escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// A C number literal: `10`, `0xFF`, `10u`, `1UL`, `0.5f`, `1e-3`.
fn number(text: &str) -> Option<Val> {
    let lower = text.to_ascii_lowercase();
    let hex = lower.starts_with("0x");
    let float = if hex {
        lower.contains('p')
    } else {
        lower.contains('.') || lower.contains('e')
    };
    if float {
        // `long double`, and hex floats, aren't read.
        if hex || lower.ends_with('l') {
            return None;
        }
        return match lower.strip_suffix('f') {
            Some(digits) => Some(Val::Float(round(digits.parse().ok()?, false), false)),
            None => Some(Val::Float(lower.parse().ok()?, true)),
        };
    }
    let digits = lower.trim_end_matches(['u', 'l']);
    let suffix = &lower[digits.len()..];
    let unsigned = suffix.contains('u');
    let long = suffix.contains('l');
    let (radix, digits) = if let Some(d) = digits.strip_prefix("0x") {
        (16, d)
    } else if let Some(d) = digits.strip_prefix("0b") {
        (2, d)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let n = i128::from_str_radix(&digits.replace('_', ""), radix).ok()?;
    // The first of C's types it fits, by how it's written.
    let candidates: &[Int] = match (unsigned, long, radix == 10) {
        (true, true, _) => &[Int::U64],
        (true, false, _) => &[Int::U32, Int::U64],
        (false, true, true) => &[Int::I64],
        (false, true, false) => &[Int::I64, Int::U64],
        (false, false, true) => &[Int::I32, Int::I64],
        (false, false, false) => &[Int::I32, Int::U32, Int::I64, Int::U64],
    };
    let int = candidates.iter().copied().find(|int| int.fit(n) == Some(n))?;
    Some(Val::Int(n, int))
}

/// The type a cast names, `(unsigned int)`: an integer type, or a float one (`true` for
/// `double`).
fn cast_type(words: &[&str]) -> Option<Result<Int, bool>> {
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
        Err(true)
    } else if has("float") {
        Err(false)
    } else if has("unsigned") && has("long") {
        Ok(Int::U64)
    } else if has("unsigned") {
        Ok(Int::U32)
    } else if has("long") {
        Ok(Int::I64)
    } else {
        Ok(Int::I32)
    })
}

struct Evaluator<'a> {
    defines: &'a HashMap<&'a str, &'a Define>,
    record: &'a dyn Fn(&str) -> Option<Record>,
    /// What each definition worked out to, once it has.
    values: HashMap<String, Option<Val>>,
    /// The definitions being worked out, which one naming itself never finishes.
    evaluating: HashSet<String>,
}

impl Evaluator<'_> {
    /// What the definition `name` stands for.
    fn value(&mut self, name: &str) -> Option<Val> {
        if let Some(value) = self.values.get(name) {
            return value.clone();
        }
        let define = *self.defines.get(name)?;
        if !self.evaluating.insert(name.to_string()) {
            return None;
        }
        let value = lex(&define.body).and_then(|tokens| {
            let mut parser = Parser { tokens, at: 0 };
            let value = self.expr(&mut parser, 0)?;
            (parser.at == parser.tokens.len()).then_some(value)
        });
        self.evaluating.remove(name);
        self.values.insert(name.to_string(), value.clone());
        value
    }

    /// An expression whose operators bind at least as tightly as `least`.
    fn expr(&mut self, p: &mut Parser, least: u8) -> Option<Val> {
        let mut lhs = self.unary(p)?;
        loop {
            let Some(&Tok::Punct(op)) = p.peek() else {
                break;
            };
            let binds = match op {
                "*" | "/" | "%" => 10,
                "+" | "-" => 9,
                "<<" | ">>" => 8,
                "&" => 5,
                "^" => 4,
                "|" => 3,
                _ => break,
            };
            if binds < least {
                break;
            }
            p.at += 1;
            let rhs = self.expr(p, binds + 1)?;
            lhs = binary(op, lhs, rhs)?;
        }
        Some(lhs)
    }

    fn unary(&mut self, p: &mut Parser) -> Option<Val> {
        match p.next()? {
            Tok::Num(text) => number(&text),
            Tok::Str(text) => Some(Val::Str(text)),
            Tok::Punct("-") => match self.unary(p)? {
                Val::Int(n, int) => Some(Val::Int(int.fit(-n)?, int)),
                Val::Float(x, double) => Some(Val::Float(-x, double)),
                _ => None,
            },
            Tok::Punct("+") => self.unary(p),
            Tok::Punct("~") => match self.unary(p)? {
                Val::Int(n, int) => Some(Val::Int(int.fit(!n)?, int)),
                _ => None,
            },
            Tok::Punct("(") => {
                // `(float)x`: a cast.
                let words: Vec<String> = (p.tokens[p.at..].iter())
                    .map_while(|t| match t {
                        Tok::Ident(word) => Some(word.clone()),
                        _ => None,
                    })
                    .collect();
                let closes = p.tokens.get(p.at + words.len()) == Some(&Tok::Punct(")"));
                if !words.is_empty() && closes {
                    let refs: Vec<&str> = words.iter().map(String::as_str).collect();
                    if let Some(ty) = cast_type(&refs) {
                        p.at += words.len() + 1;
                        let value = self.unary(p)?;
                        return match ty {
                            Ok(int) => value.to_int(int),
                            Err(double) => value.to_float(double),
                        };
                    }
                    // `(Color){ … }`: a compound literal.
                    if words.len() == 1 && p.tokens.get(p.at + 2) == Some(&Tok::Punct("{")) {
                        p.at += 2;
                        return self.compound(p, &words[0]);
                    }
                }
                let value = self.expr(p, 0)?;
                p.expect(")")?;
                Some(value)
            }
            // raylib's `CLITERAL(Color){ … }`.
            Tok::Ident(name)
                if name == "CLITERAL"
                    && p.peek() == Some(&Tok::Punct("("))
                    && matches!(p.tokens.get(p.at + 1), Some(Tok::Ident(_)))
                    && p.tokens.get(p.at + 2) == Some(&Tok::Punct(")")) =>
            {
                let Some(Tok::Ident(ty)) = p.tokens.get(p.at + 1).cloned() else {
                    unreachable!()
                };
                p.at += 3;
                self.compound(p, &ty)
            }
            Tok::Ident(name) => self.value(&name),
            Tok::Punct(_) => None,
        }
    }

    /// `{ 230, 41, 55, 255 }` after `(Color)`: the struct with those fields, in order, and
    /// zero for those left out.
    fn compound(&mut self, p: &mut Parser, ty: &str) -> Option<Val> {
        let record = (self.record)(ty)?;
        p.expect("{")?;
        let mut values = Vec::new();
        while p.peek() != Some(&Tok::Punct("}")) {
            values.push(self.expr(p, 0)?);
            if p.peek() == Some(&Tok::Punct(",")) {
                p.at += 1;
            }
        }
        p.expect("}")?;
        if values.len() > record.fields.len() {
            return None;
        }
        let fields = (record.fields.iter().enumerate())
            .map(|(i, (field, ty))| {
                let value = values.get(i).cloned().unwrap_or(Val::Int(0, Int::I32));
                Some((field.clone(), field_text(&value, ty)?))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Val::Struct(record.name, fields))
    }
}

/// `value` written for a field of som type `ty`.
fn field_text(value: &Val, ty: &str) -> Option<String> {
    let int = |bits: u32, signed: bool| -> Option<String> {
        let Val::Int(n, _) = *value else { return None };
        let fits = match signed {
            true => -(1i128 << (bits - 1)) <= n && n < 1i128 << (bits - 1),
            false => 0 <= n && n < 1i128 << bits,
        };
        fits.then(|| n.to_string())
    };
    match ty {
        "i8" => int(8, true),
        "i16" => int(16, true),
        "i32" => int(32, true),
        "i64" | "isize" => int(64, true),
        "u8" => int(8, false),
        "u16" => int(16, false),
        "u32" => int(32, false),
        "u64" | "usize" => int(64, false),
        "f32" | "f64" => {
            let double = ty == "f64";
            let Val::Float(x, _) = value.to_float(double)? else {
                return None;
            };
            float_text(x, double)
        }
        "bool" => match value {
            Val::Int(0, _) => Some("false".into()),
            Val::Int(1, _) => Some("true".into()),
            _ => None,
        },
        _ => None,
    }
}

/// `lhs op rhs`, as C works it out.
fn binary(op: &str, lhs: Val, rhs: Val) -> Option<Val> {
    match (&lhs, &rhs) {
        (Val::Int(a, ka), Val::Int(b, kb)) => {
            let (a, b) = (*a, *b);
            // A shift's type is its left side's; anything else, the wider of the two.
            let int = match op {
                "<<" | ">>" => *ka,
                _ => (*ka).max(*kb),
            };
            let n = match op {
                "+" => a.checked_add(b)?,
                "-" => a.checked_sub(b)?,
                "*" => a.checked_mul(b)?,
                "/" => a.checked_div(b)?,
                "%" => a.checked_rem(b)?,
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" | ">>" if !(0..int.bits() as i128).contains(&b) => return None,
                "<<" => a.checked_shl(b as u32)?,
                ">>" => a >> b,
                _ => return None,
            };
            Some(Val::Int(int.fit(n)?, int))
        }
        (Val::Int(..) | Val::Float(..), Val::Int(..) | Val::Float(..)) => {
            let double = matches!(lhs, Val::Float(_, true)) || matches!(rhs, Val::Float(_, true));
            let (Some(Val::Float(a, _)), Some(Val::Float(b, _))) =
                (lhs.to_float(double), rhs.to_float(double))
            else {
                return None;
            };
            let x = match op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                _ => return None,
            };
            Some(Val::Float(round(x, double), double))
        }
        _ => None,
    }
}

struct Parser {
    tokens: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Tok> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn expect(&mut self, punct: &str) -> Option<()> {
        match self.next()? {
            Tok::Punct(p) if p == punct => Some(()),
            _ => None,
        }
    }
}

/// Whether `file`, as the preprocessor names it, is the header `header`.
pub(super) fn same_file(file: &str, header: &str) -> bool {
    Path::new(file).file_name() == Path::new(header).file_name()
}

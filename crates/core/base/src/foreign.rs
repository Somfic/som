//! Other languages' syntax, and what som writes instead, so every pass reports a habit the
//! same way. Recognising a construct in context is the parser's or the resolver's job; each
//! asks a table for the word it found.

use crate::message::{Message, code};
use crate::msg;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Python,
    Java,
    CSharp,
    C,
    Cpp,
    JavaScript,
    TypeScript,
    Lua,
    Ruby,
    Go,
    Kotlin,
    Swift,
    Scala,
    Php,
    Fsharp,
    Rust,
    Zig,
}

impl Lang {
    pub fn name(self) -> &'static str {
        match self {
            Lang::Python => "Python",
            Lang::Java => "Java",
            Lang::CSharp => "C#",
            Lang::C => "C",
            Lang::Cpp => "C++",
            Lang::JavaScript => "JavaScript",
            Lang::TypeScript => "TypeScript",
            Lang::Lua => "Lua",
            Lang::Ruby => "Ruby",
            Lang::Go => "Go",
            Lang::Kotlin => "Kotlin",
            Lang::Swift => "Swift",
            Lang::Scala => "Scala",
            Lang::Php => "PHP",
            Lang::Fsharp => "F#",
            Lang::Rust => "Rust",
            Lang::Zig => "Zig",
        }
    }
}

use Lang::*;

/// `som` is empty when som writes nothing in its place: `static`, `pass`.
#[derive(Debug)]
pub struct Foreign {
    pub written: &'static str,
    pub som: &'static str,
    pub from: &'static [Lang],
}

impl Foreign {
    pub fn hint(&self) -> Message {
        let verb = if self.from.len() == 1 {
            "writes"
        } else {
            "write"
        };
        msg![
            code(self.written),
            format!(" is how {} {verb} it", languages(self.from))
        ]
    }
}

fn languages(from: &[Lang]) -> String {
    let names: Vec<&str> = from.iter().map(|l| l.name()).collect();
    match names.as_slice() {
        [] => "other languages".to_string(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

const fn f(written: &'static str, som: &'static str, from: &'static [Lang]) -> Foreign {
    Foreign { written, som, from }
}

pub struct Table(&'static [Foreign]);

impl Table {
    pub fn find(&self, written: &str) -> Option<&'static Foreign> {
        self.0.iter().find(|f| f.written == written)
    }
}

pub static LET: Table = Table(&[
    f("const", "let", &[JavaScript, TypeScript, Cpp]),
    f(
        "var",
        "let mut",
        &[JavaScript, Java, CSharp, Go, Kotlin, Swift],
    ),
    f("val", "let", &[Kotlin, Scala]),
    f("auto", "let", &[C, Cpp]),
    f("set", "let", &[]),
    f("local", "let mut", &[Lua]),
]);

pub static CONST: Table = Table(&[
    f(
        "const",
        "@const",
        &[Rust, Zig, C, Cpp, JavaScript, TypeScript],
    ),
    f("constexpr", "@const", &[Cpp]),
]);

pub static FN: Table = Table(&[
    f("def", "fn", &[Python, Ruby, Scala]),
    f("function", "fn", &[JavaScript, TypeScript, Lua, Php]),
    f("func", "fn", &[Go, Swift]),
    f("fun", "fn", &[Kotlin]),
    f("method", "fn", &[]),
    f("void", "fn", &[C, Cpp, Java, CSharp]),
]);

pub static STRUCT: Table = Table(&[f(
    "class",
    "struct",
    &[Java, CSharp, Cpp, Python, JavaScript, Kotlin, Swift],
)]);

pub static MODIFIER: Table = Table(&[
    f(
        "public",
        "pub",
        &[Java, CSharp, Cpp, TypeScript, Kotlin, Swift, Php],
    ),
    f(
        "private",
        "",
        &[Java, CSharp, Cpp, TypeScript, Kotlin, Swift, Php],
    ),
    f(
        "protected",
        "",
        &[Java, CSharp, Cpp, TypeScript, Kotlin, Php],
    ),
    f(
        "static",
        "",
        &[Java, CSharp, C, Cpp, TypeScript, Swift, Php],
    ),
]);

pub static MUT: Table = Table(&[f("mutable", "mut", &[Fsharp])]);

pub static VALUE: Table = Table(&[
    f(
        "null",
        "none",
        &[Java, CSharp, JavaScript, TypeScript, Kotlin, Php],
    ),
    f("nil", "none", &[Lua, Ruby, Go, Swift]),
    f("NULL", "none", &[C, Cpp]),
    f("nullptr", "none", &[Cpp]),
    f("None", "none", &[Python]),
    f("undefined", "none", &[JavaScript, TypeScript]),
    f("True", "true", &[Python]),
    f("False", "false", &[Python]),
    f("TRUE", "true", &[C]),
    f("FALSE", "false", &[C]),
]);

// Apart from `VALUE`: `this` only means something where there's a `self`.
pub static SELF: Table = Table(&[f(
    "this",
    "self",
    &[Java, CSharp, Cpp, JavaScript, TypeScript, Kotlin],
)]);

// An empty `som`: there's no one som type for it, so no fix.
pub static TYPE: Table = Table(&[
    f("int", "i32", &[C, Cpp, Java, CSharp, Go]),
    f("long", "i64", &[C, Cpp, Java, CSharp]),
    f("short", "i16", &[C, Cpp, Java, CSharp]),
    f("byte", "u8", &[Java, CSharp, Go]),
    f("double", "f64", &[C, Cpp, Java, CSharp]),
    f("float", "", &[C, Cpp, Java, CSharp, Python]),
    f("number", "f64", &[TypeScript]),
    f("boolean", "bool", &[Java, TypeScript]),
    f("Boolean", "bool", &[Java, Kotlin]),
    f("Integer", "i32", &[Java]),
    f("Int", "i32", &[Kotlin, Swift]),
    f("Double", "f64", &[Java, Kotlin, Swift]),
    f("String", "string", &[Java, CSharp, Kotlin, Swift]),
    f("int8", "i8", &[Go]),
    f("int16", "i16", &[Go]),
    f("int32", "i32", &[Go]),
    f("int64", "i64", &[Go]),
    f("uint", "u32", &[C, Go]),
    f("uint8", "u8", &[Go]),
    f("uint16", "u16", &[Go]),
    f("uint32", "u32", &[Go]),
    f("uint64", "u64", &[Go]),
    f("float32", "f32", &[Go]),
    f("float64", "f64", &[Go]),
    f("ArrayList", "List", &[Java]),
    f("Vec", "List", &[]),
    f("vector", "List", &[Cpp]),
    f("list", "List", &[Python]),
    f("HashMap", "Map", &[Java]),
    f("Dictionary", "Map", &[CSharp]),
    f("dict", "Map", &[Python]),
]);

pub static ELSE_IF: Table = Table(&[
    f("elif", "else if", &[Python]),
    f("elsif", "else if", &[Ruby]),
    f("elseif", "else if", &[Lua, Php]),
]);

pub static OPERATOR: Table = Table(&[
    f("not", "!", &[Python]),
    f("and", "&&", &[Python, Lua, Ruby]),
    f("or", "||", &[Python, Lua, Ruby]),
    f("===", "==", &[JavaScript, TypeScript, Php]),
    f("!==", "!=", &[JavaScript, TypeScript, Php]),
    f("~=", "!=", &[Lua]),
    f(":=", "let", &[Go]),
    f("++", "x = x + 1", &[C, Cpp, Java, CSharp, JavaScript, Go]),
    f("--", "x = x - 1", &[C, Cpp, Java, CSharp, JavaScript, Go]),
]);

pub static MEMBER: Table = Table(&[
    f("length", "len", &[JavaScript, Java, TypeScript]),
    f("size", "len", &[Java, Cpp, Kotlin, Scala]),
    f("count", "len", &[Swift]),
    f("Length", "len", &[CSharp]),
    f("Count", "len", &[CSharp]),
    f("append", "push", &[Python, Go, Swift]),
    f("add", "push", &[Java, CSharp, Kotlin]),
    f("push_back", "push", &[Cpp]),
]);

pub static LEN: Table = Table(&[f("len", "len", &[Python, Go])]);

pub static PRINT: Table = Table(&[
    f("console.log", "println", &[JavaScript, TypeScript]),
    f("System.out.println", "println", &[Java]),
    f("Console.WriteLine", "println", &[CSharp]),
    f("fmt.Println", "println", &[Go]),
    f("puts", "println", &[Ruby, C]),
]);

pub static COMMENT: Table = Table(&[f("#", "//", &[Python, Ruby]), f("--", "//", &[Lua])]);

pub static BLOCK: Table = Table(&[
    f("then", "{", &[Lua, Ruby]),
    f("do", "{", &[Lua, Ruby]),
    f("end", "}", &[Lua, Ruby]),
    f(":", "{", &[Python]),
]);

pub static STATEMENT: Table = Table(&[
    f("pass", "", &[Python]),
    f(
        "switch",
        "match",
        &[C, Cpp, Java, CSharp, JavaScript, Go, Swift],
    ),
    f("new", "", &[Java, CSharp, Cpp, JavaScript, TypeScript, Php]),
]);

pub static LAMBDA: Table = Table(&[
    f("lambda", "fn", &[Python]),
    f("=>", "fn", &[JavaScript, TypeScript, CSharp]),
    f("->", "fn", &[Fsharp, Kotlin, Java]),
    f("function", "fn", &[JavaScript, Lua, Php]),
    f("func", "fn", &[Go, Swift]),
]);

pub static RANGE: Table = Table(&[f("range", "..", &[Python])]);

pub static CONSTRUCTOR: &[&str] = &["new", "create", "init", "make", "default", "construct"];

pub fn value(word: &str) -> Option<&'static Foreign> {
    VALUE.find(word)
}
pub fn self_word(word: &str) -> Option<&'static Foreign> {
    SELF.find(word)
}
pub fn type_name(word: &str) -> Option<&'static Foreign> {
    TYPE.find(word)
}
pub fn print(path: &str) -> Option<&'static Foreign> {
    PRINT.find(path)
}
pub fn range(word: &str) -> Option<&'static Foreign> {
    RANGE.find(word)
}
pub fn member(name: &str) -> Option<&'static Foreign> {
    MEMBER.find(name)
}
pub fn len_function(name: &str) -> Option<&'static Foreign> {
    LEN.find(name)
}

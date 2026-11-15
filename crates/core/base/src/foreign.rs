//! Other languages' syntax, and what som writes instead.
//!
//! Someone coming from Python, Java or Lua writes what they know: `def`, `null`, `elif`,
//! `int x;`. These tables say, for each such word, what som writes and where it's from, so
//! every pass reports it the same way: what som writes, and a hint naming the language.
//!
//! Only the knowledge lives here. Recognising a construct in context (a `def` followed by a
//! name, a `then … end` body) is the parser's, or the resolver's, job; each asks the table
//! for the word it found.

use crate::message::{Message, code};
use crate::msg;

/// A language whose habits som recognises.
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
        }
    }
}

use Lang::*;

/// A word, or a bit of syntax, from other languages: `written`, which som writes as `som`
/// (empty when som writes nothing in its place: `static`, `pass`).
#[derive(Debug)]
pub struct Foreign {
    pub written: &'static str,
    pub som: &'static str,
    pub from: &'static [Lang],
}

impl Foreign {
    /// "`elif` is how Python writes it", or "… how Java, C# and C++ write it".
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

/// `Python`, `Java and C#`, `Java, C# and C++`.
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

fn find(table: &'static [Foreign], word: &str) -> Option<&'static Foreign> {
    table.iter().find(|f| f.written == word)
}

/// Declaring a variable: `const name = …`. `var` is mutable where it's from.
pub static LET: &[Foreign] = &[
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
];

/// Declaring a function: `def area(…)`.
pub static FN: &[Foreign] = &[
    f("def", "fn", &[Python, Ruby, Scala]),
    f("function", "fn", &[JavaScript, TypeScript, Lua, Php]),
    f("func", "fn", &[Go, Swift]),
    f("fun", "fn", &[Kotlin]),
    f("method", "fn", &[]),
    f("void", "fn", &[C, Cpp, Java, CSharp]),
];

/// Declaring a struct: `class Point`.
pub static STRUCT: &[Foreign] = &[f(
    "class",
    "struct",
    &[Java, CSharp, Cpp, Python, JavaScript, Kotlin, Swift],
)];

/// Modifiers before an item or field: `public fn`. Empty `som`: som has nothing in its place.
pub static MODIFIER: &[Foreign] = &[
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
];

/// After `let`: `let mutable total`.
pub static MUT: &[Foreign] = &[f("mutable", "mut", &[Fsharp])];

/// Values: `null` for `none`. (`this` for `self` only makes sense where there's a `self`,
/// so it's [`SELF`], apart.)
pub static VALUE: &[Foreign] = &[
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
];

/// The value a method's called on.
pub static SELF: &[Foreign] = &[f(
    "this",
    "self",
    &[Java, CSharp, Cpp, JavaScript, TypeScript, Kotlin],
)];

/// Types: `int` for `i32`. Empty `som`: there's no one som type for it, so no fix.
pub static TYPE: &[Foreign] = &[
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
    f("string", "str", &[CSharp, TypeScript, Go]),
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
];

/// `elif c:`, `elseif c then`: som's `else if`.
pub static ELSE_IF: &[Foreign] = &[
    f("elif", "else if", &[Python]),
    f("elsif", "else if", &[Ruby]),
    f("elseif", "else if", &[Lua, Php]),
];

/// Operators written as words, or as other symbols: `not x`, `a && b`, `a === b`.
pub static OPERATOR: &[Foreign] = &[
    f("not", "!", &[Python]),
    f(
        "&&",
        "and",
        &[C, Cpp, Java, CSharp, JavaScript, Go, Kotlin, Swift],
    ),
    f(
        "||",
        "or",
        &[C, Cpp, Java, CSharp, JavaScript, Go, Kotlin, Swift],
    ),
    f("===", "==", &[JavaScript, TypeScript, Php]),
    f("!==", "!=", &[JavaScript, TypeScript, Php]),
    f("~=", "!=", &[Lua]),
    f(":=", "let", &[Go]),
    f("++", "x = x + 1", &[C, Cpp, Java, CSharp, JavaScript, Go]),
    f("--", "x = x - 1", &[C, Cpp, Java, CSharp, JavaScript, Go]),
];

/// Printing a line: `console.log(…)`. Written as the path called.
pub static PRINT: &[Foreign] = &[
    f("console.log", "println", &[JavaScript, TypeScript]),
    f("System.out.println", "println", &[Java]),
    f("Console.WriteLine", "println", &[CSharp]),
    f("fmt.Println", "println", &[Go]),
    f("puts", "println", &[Ruby, C]),
];

/// Comments: `# note`, and `-- note` on a line of its own.
pub static COMMENT: &[Foreign] = &[f("#", "//", &[Python, Ruby]), f("--", "//", &[Lua])];

/// Bodies written without braces, and the words around them.
pub static BLOCK: &[Foreign] = &[
    f("then", "{", &[Lua, Ruby]),
    f("do", "{", &[Lua, Ruby]),
    f("end", "}", &[Lua, Ruby]),
    f(":", "{", &[Python]),
];

/// Statements with no som counterpart of their own.
pub static STATEMENT: &[Foreign] = &[
    f("pass", "", &[Python]),
    f(
        "switch",
        "match",
        &[C, Cpp, Java, CSharp, JavaScript, Go, Swift],
    ),
    f("new", "", &[Java, CSharp, Cpp, JavaScript, TypeScript, Php]),
];

/// Anonymous functions, which som doesn't have: `lambda x: …`, `(a, b) => …`.
pub static LAMBDA: &[Foreign] = &[
    f("lambda", "fn", &[Python]),
    f("=>", "fn", &[JavaScript, TypeScript, CSharp]),
    f("->", "fn", &[Fsharp, Kotlin, Java]),
    f("function", "fn", &[JavaScript, Lua, Php]),
    f("func", "fn", &[Go, Swift]),
];

/// Looping over numbers: `range(n)`.
pub static RANGE: &[Foreign] = &[f("range", "..", &[Python])];

/// Names other languages give a constructor: `List::new()` where `List` has `empty`.
pub static CONSTRUCTOR: &[&str] = &["new", "create", "init", "make", "default", "construct"];

pub fn let_word(word: &str) -> Option<&'static Foreign> {
    find(LET, word)
}
pub fn fn_word(word: &str) -> Option<&'static Foreign> {
    find(FN, word)
}
pub fn struct_word(word: &str) -> Option<&'static Foreign> {
    find(STRUCT, word)
}
pub fn modifier(word: &str) -> Option<&'static Foreign> {
    find(MODIFIER, word)
}
pub fn mut_word(word: &str) -> Option<&'static Foreign> {
    find(MUT, word)
}
pub fn value(word: &str) -> Option<&'static Foreign> {
    find(VALUE, word)
}
pub fn self_word(word: &str) -> Option<&'static Foreign> {
    find(SELF, word)
}
pub fn type_name(word: &str) -> Option<&'static Foreign> {
    find(TYPE, word)
}
pub fn else_if(word: &str) -> Option<&'static Foreign> {
    find(ELSE_IF, word)
}
pub fn operator(written: &str) -> Option<&'static Foreign> {
    find(OPERATOR, written)
}
pub fn print(path: &str) -> Option<&'static Foreign> {
    find(PRINT, path)
}
pub fn comment(written: &str) -> Option<&'static Foreign> {
    find(COMMENT, written)
}
pub fn block(word: &str) -> Option<&'static Foreign> {
    find(BLOCK, word)
}
pub fn statement(word: &str) -> Option<&'static Foreign> {
    find(STATEMENT, word)
}
pub fn lambda(written: &str) -> Option<&'static Foreign> {
    find(LAMBDA, written)
}
pub fn range(word: &str) -> Option<&'static Foreign> {
    find(RANGE, word)
}

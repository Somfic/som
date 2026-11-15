//! Other languages' syntax: each is reported, and applying every fix gives som that parses.

use som_base::Source;
use std::sync::Arc;

/// `source` with the fix of every error it has applied.
fn fixed(source: &str) -> String {
    let (_, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(!errors.is_empty(), "nothing reported in {source:?}");
    // (start, length, text), in the order the fixes give them.
    let mut edits: Vec<(usize, usize, String)> = errors
        .iter()
        .filter_map(|e| e.fix.as_ref())
        .flat_map(|fix| fix.edits.iter())
        .map(|(span, text)| (span.start_offset, span.length, text.clone()))
        .collect();
    // Back to front, so earlier offsets stay put. At one offset: what's replaced first, then
    // what's put in, the last given first, so they end up in the order given.
    edits.reverse();
    edits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut text = source.to_string();
    for (start, length, with) in edits {
        text.replace_range(start..start + length, &with);
    }
    text
}

/// Applying `source`'s fixes gives `expected`, which parses without errors.
#[track_caller]
fn becomes(source: &str, expected: &str) {
    let text = fixed(source);
    assert_eq!(text, expected);
    let (_, errors) = som_parser::parse(Arc::new(Source::from_raw(&*text)));
    let messages: Vec<String> = errors.iter().map(|e| e.message.plain()).collect();
    assert!(
        messages.is_empty(),
        "{text:?} still has errors: {messages:?}"
    );
}

/// `source` is reported, with no fix.
#[track_caller]
fn reported(source: &str) {
    let (_, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(!errors.is_empty(), "nothing reported in {source:?}");
}

#[test]
fn declarations() {
    becomes("const x = 1;", "let x = 1;");
    becomes("var x = 1; x = 2;", "let mut x = 1; x = 2;");
    becomes("local x = 1;", "let mut x = 1;");
    becomes("x := 5;", "let x = 5;");
    becomes("let mutable total = 0;", "let mut total = 0;");
    becomes("def area(w, h) { w * h }", "fn area(w, h) { w * h }");
    becomes("func area(w, h) { w * h }", "fn area(w, h) { w * h }");
    becomes("fun area(w, h) { w * h }", "fn area(w, h) { w * h }");
    becomes("class P { x: i32 }", "struct P { x: i32 }");
    becomes("public static void main() {}", "pub fn main() {}");
    becomes("fn k() -> void {}", "fn k() {}");
    becomes(
        "int sum(int a, int b) { a + b }",
        "fn sum(a: int, b: int) -> int { a + b }",
    );
}

#[test]
fn operators() {
    becomes("let c = a === b;", "let c = a == b;");
    becomes("let c = a !== b;", "let c = a != b;");
    becomes("let c = a ~= b;", "let c = a != b;");
    becomes("let c = a && b;", "let c = a and b;");
    becomes("let c = a || b;", "let c = a or b;");
    becomes("let c = not a;", "let c = !a;");
    becomes("i++;", "i = i + 1;");
    becomes("i--;", "i = i - 1;");
    becomes("++i;", "i = i + 1;");
}

#[test]
fn what_stays_som() {
    // `a--b` is `a - (-b)`, and `&&x` a borrow of a borrow.
    for source in ["let c = a--b;", "let r = &&x;", "let not = 1; let y = not;"] {
        let (_, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
        assert!(errors.is_empty(), "{source:?}: {errors:?}");
    }
}

#[test]
fn comments() {
    becomes("# note\nlet x = 1;", "// note\nlet x = 1;");
    becomes("-- note\nlet x = 1;", "// note\nlet x = 1;");
}

#[test]
fn loops() {
    becomes(
        "for (int i = 0; i < n; i++) { f(i); }",
        "for i in 0..n { f(i); }",
    );
    becomes(
        "for (let i = 0; i <= n; i++) { f(i); }",
        "for i in 0..n + 1 { f(i); }",
    );
    becomes(
        "for i = 1, n do\n    f(i);\nend",
        "for i in 1..n + 1 {\n    f(i);\n}",
    );
}

#[test]
fn bodies() {
    becomes(
        "fn f(x: i32) -> i32 {\n    if x > 1 then\n        return 1\n    elseif x > 0 then\n        return 2\n    else\n        return 3\n    end\n}",
        "fn f(x: i32) -> i32 {\n    if x > 1 {\n        return 1;\n    } else if x > 0 {\n        return 2;\n    } else {\n        return 3;\n    }\n}",
    );
    becomes(
        "def area(w, h):\n    let a = w * h\n    return a\n\nlet z = 1;",
        "fn area(w, h) {\n    let a = w * h;\n    return a;\n}\n\nlet z = 1;",
    );
    becomes(
        "fn f(x: i32) -> i32 {\n    if x > 1:\n        return 1\n    elif x > 0:\n        return 2\n    else:\n        return 3\n    return 0;\n}",
        "fn f(x: i32) -> i32 {\n    if x > 1 {\n        return 1;\n    }\n    else if x > 0 {\n        return 2;\n    }\n    else {\n        return 3;\n    }\n    return 0;\n}",
    );
    becomes(
        "while x > 0 do\n    x = x - 1\nend",
        "while x > 0 {\n    x = x - 1;\n}",
    );
    becomes(
        "def area(w, h)\n    w * h\nend",
        "fn area(w, h) {\n    w * h;\n}",
    );
}

#[test]
fn statements() {
    becomes("fn f() {\n    pass\n}", "fn f() {\n    \n}");
    reported("switch (x) { case 1: break; }");
    reported("let p = new P(1);");
}

#[test]
fn anonymous_functions() {
    for source in [
        "let f = lambda x: x + 1;",
        "let f = (a, b) => a + b;",
        "let f = x => x + 1;",
        "let f = function(a) { return a; };",
        "let ys = map(xs, fun x -> x + 1);",
    ] {
        let (_, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
        assert_eq!(errors.len(), 1, "{source:?}: {errors:?}");
    }
}

#[test]
fn java_classes() {
    becomes(
        "class Point {\n    private int x;\n    int y;\n}",
        "struct Point {\n    x: int,\n    y: int,\n}",
    );
}

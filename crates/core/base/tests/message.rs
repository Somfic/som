use som_base::message::{Kind, Part, code, command, func, path, ty};
use som_base::{Diagnostic, msg};

#[test]
fn plain_puts_tagged_parts_in_backticks() {
    let m = msg![
        "expected ",
        ty("i32"),
        ", found ",
        ty("bool"),
        " in ",
        path("main.som")
    ];
    assert_eq!(m.plain(), "expected `i32`, found `bool` in `main.som`");
    assert_eq!(m.to_string(), m.plain());
}

#[test]
fn colour_has_no_backticks_and_one_colour_per_kind() {
    let m = msg![func("main"), " returns ", ty("i32")];
    let coloured = m.render(true);
    assert!(!coloured.contains('`'), "{coloured:?}");
    assert!(
        coloured.contains("\x1b[38;2;137;180;250mmain"),
        "functions are blue: {coloured:?}"
    );
    assert!(
        coloured.contains("\x1b[38;2;249;226;175mi32"),
        "types are yellow: {coloured:?}"
    );
}

#[test]
fn strings_convert_to_plain_text() {
    let m: som_base::Message = "just words".into();
    assert_eq!(m.0, [Part::Text("just words".into())]);
    assert_eq!(
        msg![command("--cc"), code("use std;")].plain(),
        "`--cc``use std;`"
    );
    assert_eq!(
        som_base::message::module("som"),
        Part::Name(Kind::Module, "som".into())
    );
}

#[test]
fn diagnostics_render_plain_by_default() {
    let d = Diagnostic::error(msg!["cannot find ", func("helpr")]).with_hint(msg![
        "did you mean ",
        func("helper"),
        "?"
    ]);
    assert_eq!(
        d.to_string(),
        "error: cannot find `helpr`\n     ? did you mean `helper`?\n"
    );
    assert!(d.render(true).contains('\x1b'));
}

//! `extern "lib" from "lib.h"` imports, and hand-written declarations checked against
//! the header, with a small library compiled by the system C compiler.

use som_driver::{Error, LibraryError, LoadError, Options, check};
use std::path::Path;
use std::process::Command;

const HEADER: &str = "
// A pair of numbers
typedef struct Pair {
    int left;    // The left one
    float right; // The right one
} Pair;
typedef struct Buffer { int data[4]; } Buffer;
int demo_add(int a, int b);
Pair demo_pair(int left);
int demo_count(const char *format, ...);
int demo_sum(Buffer buffer);
// Section heading, not documentation
int AddNumbers(int a, int b); // Adds two numbers
/** Frames per second. */
int GetFPS(void);
#define DEMO_API extern
typedef struct demo_size { int w; int h; int sampleRate; } demo_size;
/*
Makes a size.

    demo_size s = demo_size_of(2);
*/
DEMO_API demo_size demo_size_of(int w);
/* Frees nothing,    */
/* in two lines.     */
DEMO_API void demo_free(void);
";

const SOURCE: &str = "
#include <stdarg.h>
#include \"demo.h\"
int demo_add(int a, int b) { return a + b; }
Pair demo_pair(int left) { Pair p = { left, 1.5f }; return p; }
int demo_count(const char *format, ...) { return 3; }
int demo_sum(Buffer buffer) { return buffer.data[0]; }
int AddNumbers(int a, int b) { return a + b; }
int GetFPS(void) { return 60; }
demo_size demo_size_of(int w) { demo_size s = { w, w, 2 * w }; return s; }
void demo_free(void) {}
";

/// A directory with `demo.h` and the shared library `demo` built from it.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("demo.h"), HEADER).unwrap();
    std::fs::write(dir.path().join("demo.c"), SOURCE).unwrap();
    let library = if cfg!(target_os = "macos") {
        "libdemo.dylib"
    } else {
        "libdemo.so"
    };
    let status = Command::new("cc")
        .args(["-shared", "-fPIC", "-o"])
        .arg(dir.path().join(library))
        .arg(dir.path().join("demo.c"))
        .status()
        .unwrap();
    assert!(status.success());
    dir
}

fn write(dir: &Path, source: &str) -> std::path::PathBuf {
    let path = dir.join("main.som");
    std::fs::write(&path, source).unwrap();
    path
}

fn kinds(result: Result<som_driver::Checked, Vec<Error>>) -> Vec<String> {
    result
        .err()
        .unwrap_or_default()
        .iter()
        .map(Error::kind)
        .collect()
}

#[test]
fn imported_functions_and_structs_can_be_used() {
    let dir = project();
    let main = write(
        dir.path(),
        "extern \"demo\" from \"demo.h\";
         fn main() -> i32 {
             let p: demo::Pair = demo::demo_pair(40);
             demo::demo_add(p.left, 2) + demo::demo_count(\"%d %d\", 1, 2) - 3
         }",
    );
    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    assert_eq!(
        som_driver::run(&checked, &exe, &[]).unwrap().code(),
        Some(42)
    );
}

#[test]
fn functions_som_cannot_express_are_left_out() {
    // `Buffer` has an array field, which som can't express, so `demo_sum` isn't imported.
    let dir = project();
    let main = write(
        dir.path(),
        "extern \"demo\" from \"demo.h\"; fn main() -> i32 { demo::demo_sum(0) }",
    );
    assert_eq!(
        kinds(check(&main, &Options::default())),
        ["UnknownFunction"]
    );
}

#[test]
fn a_missing_header_is_reported() {
    let dir = project();
    let main = write(dir.path(), "extern \"demo\" from \"nope.h\"; fn main() {}");
    match check(&main, &Options::default()).err().unwrap().as_slice() {
        [Error::Load(LoadError::HeaderNotFound { header, .. })] => assert_eq!(header, "nope.h"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn hand_written_declarations_are_checked_against_the_header() {
    let dir = project();
    let main = write(
        dir.path(),
        "extern \"demo\" {
             fn demo_add(a: i32, b: f32) -> i32;
             fn demo_pair() -> Pair;
             fn demo_count(format: &str) -> i32;
         }
         struct Pair { left: i32, right: f32 }
         fn main() {}",
    );
    let problems: Vec<String> = check(&main, &Options::default())
        .err()
        .unwrap()
        .into_iter()
        .map(|e| match e {
            Error::Library(LibraryError::HeaderMismatch {
                function, problem, ..
            }) => format!("{function}: {}", problem.plain()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        problems,
        [
            "demo_add: parameter `b` is `i32` there, not `f32`",
            "demo_pair: takes 1 parameter there, not 0",
            "demo_count: takes extra arguments (`...`) there",
        ]
    );
}

#[test]
fn matching_declarations_pass_and_any_pointer_is_fine() {
    let dir = project();
    let main = write(
        dir.path(),
        "extern \"demo\" { fn demo_add(x: i32, y: i32) -> i32; fn demo_count(format: *, ...) -> i32; }
         fn main() -> i32 { demo::demo_add(1, 2) }",
    );
    assert!(check(&main, &Options::default()).is_ok());
}

#[test]
fn a_bare_extern_imports_the_library_header() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { demo_add(40, 2) }",
    );
    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    assert_eq!(
        som_driver::run(&checked, &exe, &[]).unwrap().code(),
        Some(42)
    );

    let main = write(dir.path(), "extern \"nothing_here\"; fn main() {}");
    assert_eq!(kinds(check(&main, &Options::default())), ["HeaderNotFound"]);
}

#[test]
fn a_header_can_include_others_from_where_it_was_found() {
    // Homebrew's gc: `include/gc.h` is `#include <gc/gc.h>`, where the declarations are.
    let dir = project();
    std::fs::create_dir(dir.path().join("demo")).unwrap();
    std::fs::rename(dir.path().join("demo.h"), dir.path().join("demo/demo.h")).unwrap();
    std::fs::write(dir.path().join("demo.h"), "#include <demo/demo.h>\n").unwrap();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { demo_add(40, 2) }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let text = std::fs::read_to_string(dir.path().join(".som").join("demo.som")).unwrap();
    assert!(
        text.contains("    /// Frees nothing,\n    /// in two lines.\n    fn demo_free();"),
        "documentation comes from the included header:\n{text}"
    );
}

#[test]
fn a_bare_extern_finds_a_header_in_a_folder_named_after_it() {
    // Homebrew's miniaudio: `include/miniaudio/miniaudio.h`.
    let dir = project();
    std::fs::create_dir(dir.path().join("demo")).unwrap();
    std::fs::rename(dir.path().join("demo.h"), dir.path().join("demo/demo.h")).unwrap();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { demo_add(40, 2) }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
}

#[test]
fn a_struct_with_the_same_layout_stands_in_for_the_headers() {
    let dir = project();
    let same = "struct Two { a: i32, b: f32 } use extern \"demo\" { fn demo_pair(left: i32) -> Two; } fn main() {}";
    assert!(check(&write(dir.path(), same), &Options::default()).is_ok());

    let different = "struct Two { a: i32, b: i32 } use extern \"demo\" { fn demo_pair(left: i32) -> Two; } fn main() {}";
    assert_eq!(
        kinds(check(&write(dir.path(), different), &Options::default())),
        ["HeaderMismatch"]
    );
}

#[test]
fn hand_written_declarations_go_in_the_library_module() {
    let dir = project();
    let qualified = "extern \"demo\" { fn demo_add(a: i32, b: i32) -> i32; } fn main() -> i32 { demo::demo_add(1, 2) }";
    assert!(check(&write(dir.path(), qualified), &Options::default()).is_ok());

    let bare = "extern \"demo\" { fn demo_add(a: i32, b: i32) -> i32; } fn main() -> i32 { demo_add(1, 2) }";
    assert_eq!(
        kinds(check(&write(dir.path(), bare), &Options::default())),
        ["UnknownFunction"]
    );
}

#[test]
fn imported_names_are_snake_case_and_link_to_the_c_names() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { add_numbers(get_fps(), 2) - 20 }",
    );
    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    assert_eq!(
        som_driver::run(&checked, &exe, &[]).unwrap().code(),
        Some(42)
    );
}

#[test]
fn hand_written_snake_case_names_link_to_the_c_names() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\" { fn add_numbers(a: i32, b: i32) -> i32; } fn main() -> i32 { add_numbers(40, 2) }",
    );
    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    assert_eq!(
        som_driver::run(&checked, &exe, &[]).unwrap().code(),
        Some(42)
    );

    // Checked against the header by the name it links to.
    let wrong = write(
        dir.path(),
        "use extern \"demo\" { fn add_numbers(a: i32) -> i32; } fn main() {}",
    );
    assert_eq!(
        kinds(check(&wrong, &Options::default())),
        ["HeaderMismatch"]
    );
}

#[test]
fn imports_are_cached_in_a_readable_file_and_refreshed_when_the_header_changes() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { add_numbers(40, 2) }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));

    let cache = dir.path().join(".som").join("demo.som");
    let text = std::fs::read_to_string(&cache).unwrap();
    assert!(
        text.contains("fn add_numbers(a: i32, b: i32) -> i32;"),
        "{text}"
    );

    // Reused as long as nothing changed: a mark added to it survives.
    std::fs::write(&cache, format!("{text}// still here\n")).unwrap();
    check(&main, &Options::default()).unwrap();
    assert!(
        std::fs::read_to_string(&cache)
            .unwrap()
            .ends_with("// still here\n")
    );

    // A changed header regenerates it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(
        dir.path().join("demo.h"),
        format!("{HEADER}int demo_new(void);\n"),
    )
    .unwrap();
    check(&main, &Options::default()).unwrap();
    assert!(
        !std::fs::read_to_string(&cache)
            .unwrap()
            .contains("// still here")
    );
}

#[test]
fn imported_structs_are_pascal_case_with_snake_case_fields() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\";
         fn main() -> i32 {
             let s: DemoSize = demo_size_of(10);
             s.w + s.h + s.sample_rate + 2
         }",
    );
    let checked = check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let exe =
        som_driver::build(&checked, &dir.path().join("program"), &Default::default()).unwrap();
    assert_eq!(
        som_driver::run(&checked, &exe, &[]).unwrap().code(),
        Some(42)
    );
}

#[test]
fn header_comments_become_doc_comments() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { get_fps() }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let text = std::fs::read_to_string(dir.path().join(".som").join("demo.som")).unwrap();

    for expected in [
        "/// A pair of numbers\nstruct Pair {\n    /// The left one\n    left: i32,\n    /// The right one\n    right: f32,",
        "    /// Adds two numbers\n    fn add_numbers(",
        "    /// Frames per second.\n    fn get_fps(",
        // A `/* */` block above a macro-prefixed declaration, kept as Markdown.
        "    /// Makes a size.\n    ///\n    ///     demo_size s = demo_size_of(2);\n    fn demo_size_of(w: i32) -> DemoSize;",
        "    /// Frees nothing,\n    /// in two lines.\n    fn demo_free();",
    ] {
        assert!(text.contains(expected), "missing:\n{expected}\nin:\n{text}");
    }
    assert!(
        !text.contains("Section heading"),
        "a section heading isn't documentation:\n{text}"
    );
}

#[test]
fn checking_a_generated_file_checks_its_program() {
    let dir = project();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { demo_add(40, 2) }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    let generated = dir.path().join(".som").join("demo.som");

    // Opening it in an editor checks it; that mustn't import the header again, into
    // `.som/.som/`.
    check(&generated, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    check(&dir.path().join(".som"), &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(!dir.path().join(".som").join(".som").exists());
}

#[test]
fn importable_lists_libraries_with_headers_and_bundled_modules() {
    let dir = project();
    let found = som_driver::importable(dir.path(), &Options::default());
    assert!(found.libraries.contains(&"demo".to_string()), "{found:?}");
    assert!(found.modules.contains(&"std".to_string()));

    // Without its header, a library can't be imported with `extern "demo";`.
    std::fs::remove_file(dir.path().join("demo.h")).unwrap();
    let found = som_driver::importable(dir.path(), &Options::default());
    assert!(!found.libraries.contains(&"demo".to_string()), "{found:?}");
}

#[test]
fn origins_say_where_functions_and_structs_are_declared() {
    // `demo.h` is a wrapper; the declarations are in `demo/demo.h`, as with Homebrew's gc.
    let dir = project();
    std::fs::create_dir(dir.path().join("demo")).unwrap();
    std::fs::rename(dir.path().join("demo.h"), dir.path().join("demo/demo.h")).unwrap();
    std::fs::write(dir.path().join("demo.h"), "#include <demo/demo.h>\n").unwrap();
    let main = write(
        dir.path(),
        "use extern \"demo\"; fn main() -> i32 { demo_add(40, 2) }",
    );
    check(&main, &Options::default()).unwrap_or_else(|e| panic!("{e:?}"));

    let origins = som_driver::origins(&dir.path().join(".som").join("demo.som"));
    let header = std::fs::read_to_string(dir.path().join("demo/demo.h")).unwrap();
    let line_of = |text: &str| header.lines().position(|l| l.contains(text)).unwrap() + 1;
    for (name, declared) in [
        ("demo_add", "int demo_add("),
        ("get_fps", "int GetFPS("),
        ("Pair", "typedef struct Pair"),
    ] {
        let (file, line) = &origins[name];
        assert!(file.ends_with("demo/demo.h"), "{name}: {file:?}");
        assert_eq!(*line, line_of(declared), "{name}");
    }
}

#[test]
fn unsaved_text_is_analyzed_instead_of_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let main = write(dir.path(), "fn main() -> i32 { 1 }");
    let options = Options {
        overlay: [(main.clone(), "fn main() -> i32 { missing() }".to_string())].into(),
        ..Options::default()
    };
    assert_eq!(
        som_driver::analyze(&main, &options)
            .errors
            .iter()
            .map(Error::kind)
            .collect::<Vec<_>>(),
        ["UnknownFunction"]
    );
    assert!(
        som_driver::analyze(&main, &Options::default())
            .errors
            .is_empty()
    );
}

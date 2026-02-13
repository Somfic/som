mod common;

use common::compile_and_run;
use som::source_raw;

#[test]
fn identity() {
    let source = source_raw!(
        r#"
    extern "tests/test_ffi.so" {
        fn identity(x: i32) -> i32;
    }

    fn main() -> i32 {
        identity(123)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 123);
}

#[test]
fn return_struct_by_value() {
    let source = source_raw!(
        r#"
    struct Vec2 {
        x: i32,
        y: i32,
    }

    extern "tests/test_ffi.so" {
        fn make_vec2(x: i32, y: i32) -> Vec2;
    }

    fn main() -> i32 {
        let vec = make_vec2(1, 2);
        vec.x + vec.y
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn roundtrip() {
    let source = source_raw!(
        r#"
    struct Vec2 {
        x: i32,
        y: i32,
    }

    extern "tests/test_ffi.so" {
        fn make_vec2(x: i32, y: i32) -> Vec2;
        fn assert_vec2(vec: Vec2, expected_x: i32, expected_y: i32) -> i32;
    }

    fn main() -> i32 {
        let vec = make_vec2(1, 2);
        assert_vec2(vec, 1, 2)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

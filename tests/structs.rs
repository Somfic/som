mod common;
use common::*;
use som::source_raw;

#[test]
fn parse_struct_definition() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }
    fn main() -> i32 { 0 }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn struct_literal() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }
    fn main() -> i32 {
        let v = Vec2 { x: 1, y: 2 };
        0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn field_access() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }
    fn main() -> i32 {
        let v = Vec2 { x: 1, y: 2 };
        v.x + v.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

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

#[test]
fn return_struct_from_function() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }

    fn make_vec() -> Vec2 {
        Vec2 { x: 10, y: 20 }
    }

    fn main() -> i32 {
        let v = make_vec();
        v.x + v.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn return_struct_with_three_fields() {
    let source = source_raw!(
        r#"
    struct Vec3 { x: i32, y: i32, z: i32 }

    fn make_vec() -> Vec3 {
        Vec3 { x: 1, y: 2, z: 3 }
    }

    fn main() -> i32 {
        let v = make_vec();
        v.x + v.y + v.z
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

#[test]
fn return_struct_access_single_field() {
    let source = source_raw!(
        r#"
    struct Wrapper { value: i32 }

    fn wrap(n: i32) -> Wrapper {
        Wrapper { value: n }
    }

    fn main() -> i32 {
        let w = wrap(42);
        w.value
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn return_struct_with_pointer_field() {
    let source = source_raw!(
        r#"
    extern {
        fn malloc(size: i32) -> *;
    }

    struct MyString { ptr: *, len: i32, cap: i32 }

    fn make_string() -> MyString {
        let p = malloc(10);
        MyString { ptr: p, len: 5, cap: 10 }
    }

    fn main() -> i32 {
        let s = make_string();
        s.len
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn chained_struct_returns() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    fn origin() -> Point {
        Point { x: 0, y: 0 }
    }

    fn offset(p: Point, dx: i32, dy: i32) -> Point {
        Point { x: p.x + dx, y: p.y + dy }
    }

    fn main() -> i32 {
        let p1 = origin();
        let p2 = offset(p1, 5, 10);
        p2.x + p2.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

// ============================================================================
// Struct size and layout tests
// ============================================================================

#[test]
fn struct_single_i32() {
    // 4 bytes, should be padded to 8 for register
    let source = source_raw!(
        r#"
    struct Single { a: i32 }

    fn make() -> Single { Single { a: 42 } }

    fn main() -> i32 {
        let s = make();
        s.a
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn struct_two_i32() {
    // 8 bytes exactly, fits in one register
    let source = source_raw!(
        r#"
    struct Pair { a: i32, b: i32 }

    fn make() -> Pair { Pair { a: 10, b: 20 } }

    fn main() -> i32 {
        let p = make();
        p.a + p.b
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn struct_three_i32() {
    // 12 bytes, requires two registers
    let source = source_raw!(
        r#"
    struct Triple { a: i32, b: i32, c: i32 }

    fn make() -> Triple { Triple { a: 1, b: 2, c: 3 } }

    fn main() -> i32 {
        let t = make();
        t.a + t.b + t.c
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

#[test]
fn struct_four_i32() {
    // 16 bytes exactly, two registers
    let source = source_raw!(
        r#"
    struct Quad { a: i32, b: i32, c: i32, d: i32 }

    fn make() -> Quad { Quad { a: 1, b: 2, c: 3, d: 4 } }

    fn main() -> i32 {
        let q = make();
        q.a + q.b + q.c + q.d
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn struct_pointer_and_i32() {
    // Pointer (8 bytes) + i32 (4 bytes) = 12 bytes, two registers
    let source = source_raw!(
        r#"
    extern { fn malloc(size: i32) -> *; }

    struct PtrInt { ptr: *, val: i32 }

    fn make(n: i32) -> PtrInt {
        PtrInt { ptr: malloc(10), val: n }
    }

    fn main() -> i32 {
        let p = make(77);
        p.val
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 77);
}

#[test]
fn struct_pass_by_value_small() {
    // 8 byte struct passed as argument
    let source = source_raw!(
        r#"
    struct Pair { x: i32, y: i32 }

    fn sum(p: Pair) -> i32 {
        p.x + p.y
    }

    fn main() -> i32 {
        let p = Pair { x: 100, y: 23 };
        sum(p)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 123);
}

#[test]
fn struct_pass_by_value_large() {
    // 16 byte struct passed as argument (two registers)
    let source = source_raw!(
        r#"
    struct Quad { a: i32, b: i32, c: i32, d: i32 }

    fn sum(q: Quad) -> i32 {
        q.a + q.b + q.c + q.d
    }

    fn main() -> i32 {
        let q = Quad { a: 10, b: 20, c: 30, d: 40 };
        sum(q)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn struct_pass_and_return() {
    // Pass struct, modify, return new struct
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    fn scale(p: Point, factor: i32) -> Point {
        Point { x: p.x * factor, y: p.y * factor }
    }

    fn main() -> i32 {
        let p1 = Point { x: 3, y: 4 };
        let p2 = scale(p1, 10);
        p2.x + p2.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 70);
}

#[test]
fn struct_multiple_args() {
    // Multiple struct arguments
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }

    fn dot(a: Vec2, b: Vec2) -> i32 {
        a.x * b.x + a.y * b.y
    }

    fn main() -> i32 {
        let v1 = Vec2 { x: 2, y: 3 };
        let v2 = Vec2 { x: 4, y: 5 };
        dot(v1, v2)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 23); // 2*4 + 3*5 = 8 + 15 = 23
}

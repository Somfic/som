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

// ============================================================================
// Struct with bool fields
// ============================================================================

#[test]
fn struct_bool_field() {
    let source = source_raw!(
        r#"
    struct S { flag: bool }
    fn main() -> i32 {
        let s = S { flag: true };
        1 if s.flag else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_bool_field_false() {
    let source = source_raw!(
        r#"
    struct S { flag: bool }
    fn main() -> i32 {
        let s = S { flag: false };
        1 if s.flag else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn struct_mixed_fields() {
    let source = source_raw!(
        r#"
    struct S { val: i32, flag: bool }
    fn main() -> i32 {
        let s = S { val: 42, flag: true };
        s.val if s.flag else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

// ============================================================================
// Struct with pointer field access
// ============================================================================

#[test]
fn struct_pointer_field_len() {
    let source = source_raw!(
        r#"
    extern { fn malloc(size: i32) -> *; }
    struct Buf { ptr: *, len: i32 }
    fn main() -> i32 {
        let b = Buf { ptr: malloc(8), len: 8 };
        b.len
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 8);
}

// ============================================================================
// Nested struct construction
// ============================================================================

#[test]
fn struct_from_other_fields() {
    let source = source_raw!(
        r#"
    struct A { x: i32 }
    struct B { y: i32 }
    fn main() -> i32 {
        let a = A { x: 3 };
        let b = B { y: a.x + 1 };
        b.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 4);
}

#[test]
fn struct_chain() {
    let source = source_raw!(
        r#"
    struct A { x: i32 }
    struct B { y: i32 }

    fn extract(a: A) -> i32 { a.x * 2 }

    fn main() -> i32 {
        let a = A { x: 5 };
        let val = extract(a);
        let b = B { y: val + 3 };
        b.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 13);
}

// ============================================================================
// Struct in conditional expressions
// ============================================================================

#[test]
fn struct_conditional_true() {
    let source = source_raw!(
        r#"
    struct P { x: i32 }
    fn main() -> i32 {
        let p = P { x: 1 } if true else P { x: 2 };
        p.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_conditional_false() {
    let source = source_raw!(
        r#"
    struct P { x: i32 }
    fn main() -> i32 {
        let p = P { x: 1 } if false else P { x: 2 };
        p.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn struct_field_in_conditional() {
    let source = source_raw!(
        r#"
    struct P { x: i32 }
    fn main() -> i32 {
        let p = P { x: 5 };
        1 if p.x > 3 else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// ============================================================================
// Struct field used in arithmetic
// ============================================================================

#[test]
fn struct_field_add() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 10, y: 20 };
        p.x + p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn struct_field_multiply() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 3, y: 4 };
        p.x * p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 12);
}

#[test]
fn struct_field_subtract() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 50, y: 30 };
        p.x - p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20);
}

#[test]
fn struct_field_modulo() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 17, y: 5 };
        let rem = p.x - (p.x / p.y) * p.y;
        rem
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

// ============================================================================
// Struct with many fields
// ============================================================================

#[test]
fn struct_five_fields() {
    let source = source_raw!(
        r#"
    struct S { a: i32, b: i32, c: i32, d: i32, e: i32 }
    fn main() -> i32 {
        let s = S { a: 1, b: 2, c: 3, d: 4, e: 5 };
        s.a + s.b + s.c + s.d + s.e
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn struct_six_fields() {
    let source = source_raw!(
        r#"
    struct S { a: i32, b: i32, c: i32, d: i32, e: i32, f: i32 }
    fn main() -> i32 {
        let s = S { a: 10, b: 20, c: 30, d: 40, e: 50, f: 60 };
        s.a + s.f
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 70);
}

// ============================================================================
// Struct passed to multiple functions
// ============================================================================

#[test]
fn struct_two_functions() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    fn get_x(p: P) -> i32 { p.x }
    fn get_y(p: P) -> i32 { p.y }

    fn main() -> i32 {
        let p1 = P { x: 10, y: 20 };
        let p2 = P { x: 10, y: 20 };
        get_x(p1) + get_y(p2)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn struct_function_chain() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    fn add_x(p: P) -> i32 { p.x + 1 }
    fn double(n: i32) -> i32 { n * 2 }

    fn main() -> i32 {
        let p = P { x: 10, y: 0 };
        double(add_x(p))
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 22);
}

// ============================================================================
// Struct returned from conditional
// ============================================================================

#[test]
fn struct_returned_from_function_conditional() {
    let source = source_raw!(
        r#"
    struct P { x: i32 }

    fn make(b: bool) -> P {
        P { x: 1 } if b else P { x: 2 }
    }

    fn main() -> i32 {
        let p = make(true);
        p.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_returned_false() {
    let source = source_raw!(
        r#"
    struct P { x: i32 }

    fn make(b: bool) -> P {
        P { x: 1 } if b else P { x: 2 }
    }

    fn main() -> i32 {
        let p = make(false);
        p.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

// ============================================================================
// Struct field comparison
// ============================================================================

#[test]
fn struct_field_equal() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 5, y: 5 };
        1 if p.x == p.y else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_field_greater() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 10, y: 3 };
        1 if p.x > p.y else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_field_less() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p = P { x: 2, y: 8 };
        1 if p.x < p.y else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// ============================================================================
// More struct patterns
// ============================================================================

#[test]
fn struct_default_pattern() {
    let source = source_raw!(
        r#"
    struct Config { width: i32, height: i32 }

    fn default_config() -> Config {
        Config { width: 80, height: 24 }
    }

    fn main() -> i32 {
        let c = default_config();
        c.height
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 24);
}

#[test]
fn struct_update_pattern() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p1 = P { x: 5, y: 10 };
        let p2 = P { x: 100, y: p1.y };
        p2.x + p2.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 110);
}

#[test]
fn struct_as_accumulator() {
    let source = source_raw!(
        r#"
    struct Acc { total: i32 }

    fn step(a: Acc, i: i32) -> Acc {
        Acc { total: a.total + i }
    }

    fn main() -> i32 {
        let a0 = Acc { total: 0 };
        let a1 = step(a0, 1);
        let a2 = step(a1, 2);
        let a3 = step(a2, 3);
        let a4 = step(a3, 4);
        a4.total
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10); // 0+1+2+3+4 = 10
}

#[test]
fn struct_with_string_field() {
    let source = source_raw!(
        r#"
    struct Msg { text: &str, code: i32 }
    fn main() -> i32 {
        let m = Msg { text: "hello", code: 42 };
        m.code
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn struct_multiple_same_type() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let a = P { x: 1, y: 2 };
        let b = P { x: 3, y: 4 };
        let c = P { x: 5, y: 6 };
        a.x + b.x + c.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 9);
}

#[test]
fn struct_function_takes_two() {
    let source = source_raw!(
        r#"
    struct A { x: i32 }
    struct B { y: i32 }

    fn combine(a: A, b: B) -> i32 {
        a.x + b.y
    }

    fn main() -> i32 {
        let a = A { x: 15 };
        let b = B { y: 27 };
        combine(a, b)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn struct_field_as_loop_bound() {
    let source = source_raw!(
        r#"
    struct Config { limit: i32 }
    fn main() -> i32 {
        let c = Config { limit: 7 };
        let mut i = 0;
        let mut sum = 0;
        while i < c.limit {
            sum = sum + 1;
            i = i + 1;
        }
        sum
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn struct_returned_by_helper() {
    let source = source_raw!(
        r#"
    struct Pair { a: i32, b: i32 }

    fn make_pair(x: i32, y: i32) -> Pair {
        Pair { a: x, b: y }
    }

    fn main() -> i32 {
        let p = make_pair(11, 22);
        p.a + p.b
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 33);
}

// ============================================================================
// Larger struct operations
// ============================================================================

#[test]
fn struct_distance_squared() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let p1 = P { x: 1, y: 2 };
        let p2 = P { x: 4, y: 6 };
        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        dx * dx + dy * dy
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 25); // 3^2 + 4^2 = 9 + 16 = 25
}

#[test]
fn struct_add_two_points() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    fn add(a: P, b: P) -> P {
        P { x: a.x + b.x, y: a.y + b.y }
    }

    fn main() -> i32 {
        let a = P { x: 10, y: 20 };
        let b = P { x: 30, y: 40 };
        let c = add(a, b);
        c.x + c.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn struct_negate() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    fn neg(p: P) -> P {
        P { x: 0 - p.x, y: 0 - p.y }
    }

    fn main() -> i32 {
        let p = P { x: 3, y: 4 };
        let n = neg(p);
        0 - n.x + (0 - n.y)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7); // 0 - (-3) + (0 - (-4)) = 3 + 4 = 7
}

#[test]
fn struct_area_rectangle() {
    let source = source_raw!(
        r#"
    struct Rect { x: i32, y: i32, w: i32, h: i32 }
    fn main() -> i32 {
        let r = Rect { x: 0, y: 0, w: 12, h: 10 };
        r.w * r.h
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 120);
}

#[test]
fn struct_wrapper_unwrap() {
    let source = source_raw!(
        r#"
    struct W { value: i32 }

    fn unwrap(w: W) -> i32 { w.value }

    fn main() -> i32 {
        unwrap(W { value: 42 })
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn struct_single_field_arithmetic() {
    let source = source_raw!(
        r#"
    struct N { v: i32 }
    fn main() -> i32 {
        let a = N { v: 10 };
        let b = N { v: 20 };
        a.v + b.v
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn struct_three_field_mixed() {
    let source = source_raw!(
        r#"
    struct S { a: i32, b: i32, c: bool }
    fn main() -> i32 {
        let s = S { a: 10, b: 20, c: true };
        (s.a + s.b) if s.c else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn struct_returned_from_while() {
    let source = source_raw!(
        r#"
    struct V { val: i32 }

    fn make_at(n: i32) -> V {
        V { val: n }
    }

    fn main() -> i32 {
        let v = make_at(9);
        v.val
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 9);
}

#[test]
fn struct_compare_fields_of_two() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }
    fn main() -> i32 {
        let a = P { x: 5, y: 10 };
        let b = P { x: 3, y: 12 };
        1 if a.x > b.x else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn struct_field_as_function_arg() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    fn add(a: i32, b: i32) -> i32 { a + b }

    fn main() -> i32 {
        let p = P { x: 17, y: 25 };
        add(p.x, p.y)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

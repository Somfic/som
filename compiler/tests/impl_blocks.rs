mod common;
use common::*;
use som::source_raw;

// ============================================================================
// Basic impl blocks
// ============================================================================

#[test]
fn basic_method_call() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn sum(self: Point) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = Point { x: 3, y: 4 };
        p.sum()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn method_with_args() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn add(self: Point, dx: i32, dy: i32) -> i32 {
            self.x + dx + self.y + dy
        }
    }

    fn main() -> i32 {
        let p = Point { x: 10, y: 20 };
        p.add(5, 15)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 50);
}

#[test]
fn multiple_methods() {
    let source = source_raw!(
        r#"
    struct Rect { w: i32, h: i32 }

    impl Rect {
        fn area(self: Rect) -> i32 {
            self.w * self.h
        }

        fn perimeter(self: Rect) -> i32 {
            self.w + self.w + self.h + self.h
        }
    }

    fn main() -> i32 {
        let r1 = Rect { w: 3, h: 4 };
        let r2 = Rect { w: 3, h: 4 };
        r1.area() + r2.perimeter()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 26); // area=12, perimeter=14
}

#[test]
fn method_returns_struct() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn scale(self: Point, factor: i32) -> Point {
            Point { x: self.x * factor, y: self.y * factor }
        }
    }

    fn main() -> i32 {
        let p = Point { x: 2, y: 3 };
        let p2 = p.scale(5);
        p2.x + p2.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 25); // 10 + 15
}

#[test]
fn method_with_struct_arg() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }

    impl Vec2 {
        fn dot(self: Vec2, other: Vec2) -> i32 {
            self.x * other.x + self.y * other.y
        }
    }

    fn main() -> i32 {
        let a = Vec2 { x: 2, y: 3 };
        let b = Vec2 { x: 4, y: 5 };
        a.dot(b)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 23); // 2*4 + 3*5
}

// ============================================================================
// Impl blocks with different struct sizes
// ============================================================================

#[test]
fn method_on_single_field_struct() {
    let source = source_raw!(
        r#"
    struct Wrapper { value: i32 }

    impl Wrapper {
        fn get(self: Wrapper) -> i32 {
            self.value
        }
    }

    fn main() -> i32 {
        let w = Wrapper { value: 42 };
        w.get()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn method_on_three_field_struct() {
    let source = source_raw!(
        r#"
    struct Vec3 { x: i32, y: i32, z: i32 }

    impl Vec3 {
        fn sum(self: Vec3) -> i32 {
            self.x + self.y + self.z
        }
    }

    fn main() -> i32 {
        let v = Vec3 { x: 1, y: 2, z: 3 };
        v.sum()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 6);
}

// ============================================================================
// Impl blocks combined with free functions
// ============================================================================

#[test]
fn method_and_free_function() {
    let source = source_raw!(
        r#"
    struct Counter { n: i32 }

    impl Counter {
        fn value(self: Counter) -> i32 {
            self.n
        }
    }

    fn double(x: i32) -> i32 {
        x + x
    }

    fn main() -> i32 {
        let c = Counter { n: 21 };
        double(c.value())
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

// ============================================================================
// Type checking error cases
// ============================================================================

#[test]
fn method_wrong_arg_count() {
    let typed_ast = test_type_check(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn sum(self: Point) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = Point { x: 1, y: 2 };
        p.sum(42)
    }
    "#,
    );

    assert!(
        !typed_ast.errors.is_empty(),
        "should have a type error for wrong arg count"
    );
}

#[test]
fn unknown_method() {
    let typed_ast = test_type_check(
        r#"
    struct Point { x: i32, y: i32 }

    fn main() -> i32 {
        let p = Point { x: 1, y: 2 };
        p.nonexistent()
    }
    "#,
    );

    assert!(
        !typed_ast.errors.is_empty(),
        "should have a type error for unknown method"
    );
}

// ============================================================================
// Static method calls (StructName::method syntax)
// ============================================================================

#[test]
fn static_method_call() {
    let source = source_raw!(
        r#"
    struct Counter { n: i32 }

    impl Counter {
        fn zero() -> Counter {
            Counter { n: 0 }
        }
    }

    fn main() -> i32 {
        let c = Counter::zero();
        c.n
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn static_method_with_args() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn new(x: i32, y: i32) -> Point {
            Point { x: x, y: y }
        }
    }

    fn main() -> i32 {
        let p = Point::new(10, 20);
        p.x + p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30);
}

#[test]
fn static_and_instance_methods() {
    let source = source_raw!(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn origin() -> Point {
            Point { x: 0, y: 0 }
        }

        fn sum(self: Point) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = Point { x: 5, y: 7 };
        p.sum()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 12);
}

// ============================================================================
// Parsing
// ============================================================================

#[test]
fn parse_impl_block() {
    let (ast, errors) = test_parse(
        r#"
    struct Point { x: i32, y: i32 }

    impl Point {
        fn sum(self: Point) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 { 0 }
    "#,
    );

    assert!(
        errors.is_empty(),
        "should parse without errors: {:?}",
        errors
    );
    assert!(
        ast.func_registry.contains_key("Point::sum"),
        "method should be registered as Point::sum, got: {:?}",
        ast.func_registry.keys().collect::<Vec<_>>()
    );
}

#[test]
fn parse_method_call() {
    let (_, errors) = test_parse(
        r#"
    struct Point { x: i32, y: i32 }
    impl Point {
        fn get_x(self: Point) -> i32 { self.x }
    }
    let p = Point { x: 1, y: 2 };
    p.get_x()
    "#,
    );

    assert!(
        errors.is_empty(),
        "should parse without errors: {:?}",
        errors
    );
}

// ============================================================================
// Methods returning bool
// ============================================================================

#[test]
fn method_returns_bool_true() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn is_positive(self: S) -> bool {
            self.x > 0
        }
    }

    fn main() -> i32 {
        let s = S { x: 5 };
        1 if s.is_positive() else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn method_returns_bool_false() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn is_large(self: S) -> bool {
            self.x > 100
        }
    }

    fn main() -> i32 {
        let s = S { x: 5 };
        1 if s.is_large() else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn method_is_zero() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn is_zero(self: S) -> bool {
            self.x == 0
        }
    }

    fn main() -> i32 {
        let s = S { x: 0 };
        1 if s.is_zero() else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

// ============================================================================
// Methods with conditional logic
// ============================================================================

#[test]
fn method_with_conditional() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn abs(self: S) -> i32 {
            self.x if self.x > 0 else 0 - self.x
        }
    }

    fn main() -> i32 {
        let s = S { x: 0 - 7 };
        s.abs()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn method_max_field() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn max(self: P) -> i32 {
            self.x if self.x > self.y else self.y
        }
    }

    fn main() -> i32 {
        let p = P { x: 3, y: 9 };
        p.max()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 9);
}

#[test]
fn method_min_field() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn min(self: P) -> i32 {
            self.x if self.x < self.y else self.y
        }
    }

    fn main() -> i32 {
        let p = P { x: 3, y: 9 };
        p.min()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

// ============================================================================
// Methods calling other methods via free functions
// ============================================================================

#[test]
fn method_uses_free_function() {
    let source = source_raw!(
        r#"
    fn double(x: i32) -> i32 {
        x * 2
    }

    struct S { x: i32 }

    impl S {
        fn doubled(self: S) -> i32 {
            double(self.x)
        }
    }

    fn main() -> i32 {
        let s = S { x: 11 };
        s.doubled()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 22);
}

#[test]
fn free_function_calls_method() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn get(self: S) -> i32 {
            self.x
        }
    }

    fn extract(s: S) -> i32 {
        s.get()
    }

    fn main() -> i32 {
        let s = S { x: 33 };
        extract(s)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 33);
}

// ============================================================================
// Multiple impl blocks pattern
// ============================================================================

#[test]
fn two_structs_with_impl() {
    let source = source_raw!(
        r#"
    struct A { val: i32 }
    struct B { val: i32 }

    impl A {
        fn get(self: A) -> i32 { self.val }
    }

    impl B {
        fn get(self: B) -> i32 { self.val * 2 }
    }

    fn main() -> i32 {
        let a = A { val: 10 };
        let b = B { val: 10 };
        a.get() + b.get()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 30); // 10 + 20
}

#[test]
fn struct_methods_interact() {
    let source = source_raw!(
        r#"
    struct Offset { dx: i32, dy: i32 }
    struct Point { x: i32, y: i32 }

    impl Point {
        fn apply(self: Point, o: Offset) -> i32 {
            self.x + o.dx + self.y + o.dy
        }
    }

    fn main() -> i32 {
        let p = Point { x: 10, y: 20 };
        let o = Offset { dx: 1, dy: 2 };
        p.apply(o)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 33);
}

// ============================================================================
// Static constructor patterns
// ============================================================================

#[test]
fn static_new_default() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn new() -> S {
            S { x: 0 }
        }
    }

    fn main() -> i32 {
        let s = S::new();
        s.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn static_new_with_value() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn new(v: i32) -> S {
            S { x: v }
        }
    }

    fn main() -> i32 {
        let s = S::new(77);
        s.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 77);
}

#[test]
fn static_from_pair() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn from_pair(x: i32, y: i32) -> P {
            P { x: x, y: y }
        }
    }

    fn main() -> i32 {
        let p = P::from_pair(15, 25);
        p.x + p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 40);
}

#[test]
fn static_and_instance_combined() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn new(v: i32) -> S {
            S { x: v }
        }

        fn get(self: S) -> i32 {
            self.x
        }
    }

    fn main() -> i32 {
        let s = S::new(55);
        s.get()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55);
}

// ============================================================================
// Method taking multiple struct args
// ============================================================================

#[test]
fn method_takes_struct_arg() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn add(self: P, other: P) -> P {
            P { x: self.x + other.x, y: self.y + other.y }
        }
    }

    fn main() -> i32 {
        let a = P { x: 3, y: 4 };
        let b = P { x: 7, y: 6 };
        let c = a.add(b);
        c.x + c.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 20); // (3+7) + (4+6)
}

#[test]
fn method_dot_product() {
    let source = source_raw!(
        r#"
    struct V { x: i32, y: i32 }

    impl V {
        fn dot(self: V, other: V) -> i32 {
            self.x * other.x + self.y * other.y
        }
    }

    fn main() -> i32 {
        let a = V { x: 3, y: 4 };
        let b = V { x: 5, y: 6 };
        a.dot(b)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 39); // 3*5 + 4*6
}

// ============================================================================
// Method with while loop
// ============================================================================

#[test]
fn method_with_while() {
    let source = source_raw!(
        r#"
    struct C { n: i32 }

    impl C {
        fn countdown(self: C) -> i32 {
            let mut n = self.n;
            let mut count = 0;
            while n > 0 {
                n = n - 1;
                count = count + 1;
            }
            count
        }
    }

    fn main() -> i32 {
        let c = C { n: 10 };
        c.countdown()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 10);
}

#[test]
fn method_iterative_sum() {
    let source = source_raw!(
        r#"
    struct S { n: i32 }

    impl S {
        fn sum_to_n(self: S) -> i32 {
            let mut total = 0;
            let mut i = 1;
            while i <= self.n {
                total = total + i;
                i = i + 1;
            }
            total
        }
    }

    fn main() -> i32 {
        let s = S { n: 10 };
        s.sum_to_n()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 55); // 1+2+...+10
}

// ============================================================================
// Method with complex arithmetic
// ============================================================================

#[test]
fn method_complex_calc() {
    let source = source_raw!(
        r#"
    struct S { a: i32, b: i32, c: i32 }

    impl S {
        fn calc(self: S) -> i32 {
            (self.a + self.b) * self.c
        }
    }

    fn main() -> i32 {
        let s = S { a: 3, b: 4, c: 5 };
        s.calc()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 35); // (3+4)*5
}

#[test]
fn method_distance_approx() {
    let source = source_raw!(
        r#"
    fn abs_diff(a: i32, b: i32) -> i32 {
        a - b if a > b else b - a
    }

    struct P { x: i32, y: i32 }

    impl P {
        fn manhattan(self: P, other: P) -> i32 {
            abs_diff(self.x, other.x) + abs_diff(self.y, other.y)
        }
    }

    fn main() -> i32 {
        let a = P { x: 1, y: 2 };
        let b = P { x: 4, y: 6 };
        a.manhattan(b)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7); // |1-4| + |2-6| = 3 + 4
}

// ============================================================================
// Type errors for method arg types
// ============================================================================

#[test]
fn method_wrong_arg_type_bool() {
    let typed_ast = test_type_check(
        r#"
    struct S { x: i32 }

    impl S {
        fn add(self: S, n: i32) -> i32 {
            self.x + n
        }
    }

    fn main() -> i32 {
        let s = S { x: 1 };
        s.add(true)
    }
    "#,
    );

    assert!(
        !typed_ast.errors.is_empty(),
        "should have a type error for passing bool where i32 expected"
    );
}

#[test]
fn method_wrong_return_used() {
    let typed_ast = test_type_check(
        r#"
    struct S { x: i32 }

    impl S {
        fn get(self: S) -> i32 {
            self.x
        }
    }

    fn main() -> bool {
        let s = S { x: 1 };
        s.get()
    }
    "#,
    );

    assert!(
        !typed_ast.errors.is_empty(),
        "should have a type error for returning i32 where bool expected"
    );
}

#[test]
fn method_missing_method() {
    let typed_ast = test_type_check(
        r#"
    struct S { x: i32 }

    impl S {
        fn get(self: S) -> i32 {
            self.x
        }
    }

    fn main() -> i32 {
        let s = S { x: 1 };
        s.missing()
    }
    "#,
    );

    assert!(
        !typed_ast.errors.is_empty(),
        "should have a type error for calling nonexistent method"
    );
}

// ============================================================================
// Chained method results through variables
// ============================================================================

#[test]
fn method_result_in_let() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn sum(self: P) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = P { x: 10, y: 20 };
        let x = p.sum();
        x + 1
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 31);
}

#[test]
fn method_result_passed_to_function() {
    let source = source_raw!(
        r#"
    fn double(x: i32) -> i32 { x * 2 }

    struct P { x: i32, y: i32 }

    impl P {
        fn sum(self: P) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = P { x: 5, y: 7 };
        double(p.sum())
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 24); // (5+7)*2
}

#[test]
fn method_result_in_conditional() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn sum(self: P) -> i32 {
            self.x + self.y
        }
    }

    fn main() -> i32 {
        let p = P { x: 3, y: 4 };
        p.sum() if true else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn two_method_calls_combined() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn get_x(self: P) -> i32 { self.x }
        fn get_y(self: P) -> i32 { self.y }
    }

    fn main() -> i32 {
        let p1 = P { x: 10, y: 20 };
        let p2 = P { x: 30, y: 40 };
        p1.get_x() + p2.get_y()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 50); // 10 + 40
}

// ============================================================================
// More patterns
// ============================================================================

#[test]
fn method_on_four_field_struct() {
    let source = source_raw!(
        r#"
    struct R { x: i32, y: i32, w: i32, h: i32 }

    impl R {
        fn area(self: R) -> i32 {
            self.w * self.h
        }
    }

    fn main() -> i32 {
        let r = R { x: 0, y: 0, w: 7, h: 8 };
        r.area()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 56);
}

#[test]
fn method_returns_same_struct() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn doubled(self: P) -> P {
            P { x: self.x * 2, y: self.y * 2 }
        }
    }

    fn main() -> i32 {
        let p = P { x: 3, y: 4 };
        let p2 = p.doubled();
        p2.x + p2.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 14); // 6 + 8
}

#[test]
fn static_method_returns_different_value() {
    let source = source_raw!(
        r#"
    struct P { x: i32, y: i32 }

    impl P {
        fn unit() -> P {
            P { x: 1, y: 1 }
        }
    }

    fn main() -> i32 {
        let p = P::unit();
        p.x + p.y
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2);
}

#[test]
fn method_with_three_args() {
    let source = source_raw!(
        r#"
    struct S { base: i32 }

    impl S {
        fn combine(self: S, a: i32, b: i32, c: i32) -> i32 {
            self.base + a + b + c
        }
    }

    fn main() -> i32 {
        let s = S { base: 10 };
        s.combine(20, 30, 40)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn method_compares_fields() {
    let source = source_raw!(
        r#"
    struct R { w: i32, h: i32 }

    impl R {
        fn is_square(self: R) -> bool {
            self.w == self.h
        }
    }

    fn main() -> i32 {
        let r = R { w: 5, h: 5 };
        1 if r.is_square() else 0
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn method_modulo() {
    let source = source_raw!(
        r#"
    struct S { value: i32 }

    impl S {
        fn remainder(self: S, divisor: i32) -> i32 {
            self.value % divisor
        }
    }

    fn main() -> i32 {
        let s = S { value: 17 };
        s.remainder(5)
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 2); // 17 % 5
}

#[test]
fn static_method_chain() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn new(v: i32) -> S {
            S { x: v }
        }

        fn triple(self: S) -> i32 {
            self.x * 3
        }
    }

    fn main() -> i32 {
        let s = S::new(9);
        s.triple()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 27);
}

#[test]
fn method_on_wrapper_type() {
    let source = source_raw!(
        r#"
    struct Wrapper { inner: i32 }

    impl Wrapper {
        fn get(self: Wrapper) -> i32 {
            self.inner
        }

        fn add(self: Wrapper, n: i32) -> Wrapper {
            Wrapper { inner: self.inner + n }
        }
    }

    fn main() -> i32 {
        let w = Wrapper { inner: 10 };
        let w2 = w.add(5);
        w2.get()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn two_methods_same_struct() {
    let source = source_raw!(
        r#"
    struct S { x: i32, y: i32 }

    impl S {
        fn get_x(self: S) -> i32 { self.x }
        fn get_y(self: S) -> i32 { self.y }
    }

    fn main() -> i32 {
        let s1 = S { x: 11, y: 22 };
        let s2 = S { x: 11, y: 22 };
        s1.get_x() + s2.get_y()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 33); // 11 + 22
}

#[test]
fn method_with_if_statement() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn clamp(self: S) -> i32 {
            let mut result = self.x;
            if self.x > 100 {
                result = 100;
            }
            if self.x < 0 {
                result = 0;
            }
            result
        }
    }

    fn main() -> i32 {
        let s = S { x: 200 };
        s.clamp()
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 100);
}

#[test]
fn static_method_with_conditional() {
    let source = source_raw!(
        r#"
    struct S { x: i32 }

    impl S {
        fn make(positive: bool) -> S {
            S { x: 1 } if positive else S { x: 0 }
        }
    }

    fn main() -> i32 {
        let s = S::make(true);
        s.x
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn method_result_as_loop_condition() {
    let source = source_raw!(
        r#"
    struct Counter { n: i32 }

    impl Counter {
        fn is_positive(self: Counter) -> bool {
            self.n > 0
        }
    }

    fn main() -> i32 {
        let mut val = 5;
        let mut total = 0;
        while val > 0 {
            let c = Counter { n: val };
            if c.is_positive() {
                total = total + val;
            }
            val = val - 1;
        }
        total
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 15); // 5+4+3+2+1
}

#[test]
fn comprehensive_impl_test() {
    let source = source_raw!(
        r#"
    struct Vec2 { x: i32, y: i32 }

    impl Vec2 {
        fn new(x: i32, y: i32) -> Vec2 {
            Vec2 { x: x, y: y }
        }

        fn sum(self: Vec2) -> i32 {
            self.x + self.y
        }

        fn scale(self: Vec2, factor: i32) -> Vec2 {
            Vec2 { x: self.x * factor, y: self.y * factor }
        }

        fn is_origin(self: Vec2) -> bool {
            self.x == 0
        }
    }

    fn add_vecs(a: Vec2, b: Vec2) -> Vec2 {
        Vec2 { x: a.x + b.x, y: a.y + b.y }
    }

    fn main() -> i32 {
        let v1 = Vec2::new(3, 4);
        let v2 = v1.scale(2);
        let v3 = Vec2::new(1, 1);
        let v4 = add_vecs(v2, v3);
        let result = v4.sum();
        let origin = Vec2::new(0, 0);
        let bonus = 0 if origin.is_origin() else 100;
        result + bonus
    }
    "#
    );

    let code = compile_and_run(source);
    assert_eq!(code, 16); // v2=(6,8), v4=(7,9), sum=16, origin.x==0 so bonus=0, total=16
}

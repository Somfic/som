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

    assert!(errors.is_empty(), "should parse without errors: {:?}", errors);
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

    assert!(errors.is_empty(), "should parse without errors: {:?}", errors);
}

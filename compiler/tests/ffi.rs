mod common;

use common::compile_and_run;
use som::source_raw;

#[test]
fn libc_abs() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 42)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn libc_strlen() {
    let source = source_raw!(
        r#"
    extern {
        fn strlen(s: &str) -> i32;
    }

    fn main() -> i32 {
        strlen("hello")
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn libc_malloc_free() {
    let source = source_raw!(
        r#"
    extern {
        fn malloc(size: i32) -> *;
        fn free(ptr: *);
    }

    fn main() -> i32 {
        let ptr = malloc(100);
        free(ptr);
        0
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn libc_memset() {
    let source = source_raw!(
        r#"
    extern {
        fn malloc(size: i32) -> *;
        fn memset(ptr: *, value: i32, size: i32) -> *;
        fn free(ptr: *);
    }

    fn main() -> i32 {
        let ptr = malloc(10);
        memset(ptr, 65, 10);
        free(ptr);
        0
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn libc_multiple_calls() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 10) + abs(0 - 20) + abs(30)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 60);
}

#[test]
fn libc_abs_positive() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(42)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn libc_abs_zero() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn libc_strlen_empty() {
    let source = source_raw!(
        r#"
    extern {
        fn strlen(s: &str) -> i32;
    }

    fn main() -> i32 {
        strlen("")
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn libc_strlen_longer() {
    let source = source_raw!(
        r#"
    extern {
        fn strlen(s: &str) -> i32;
    }

    fn main() -> i32 {
        strlen("hello world")
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 11);
}

#[test]
fn libc_multiple_malloc_free() {
    let source = source_raw!(
        r#"
    extern {
        fn malloc(size: i32) -> *;
        fn free(ptr: *);
    }

    fn main() -> i32 {
        let a = malloc(64);
        let b = malloc(128);
        let c = malloc(256);
        free(a);
        free(b);
        free(c);
        0
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn libc_abs_in_expression() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 10) + abs(0 - 5)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 15);
}

#[test]
fn libc_abs_with_conditional() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 1) if true else 0
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 1);
}

#[test]
fn libc_abs_in_while() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        let mut sum = 0;
        let mut i = 0;
        while i < 3 {
            sum = sum + abs(0 - 1);
            i = i + 1;
        }
        sum
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 3);
}

#[test]
fn libc_strlen_multiple() {
    let source = source_raw!(
        r#"
    extern {
        fn strlen(s: &str) -> i32;
    }

    fn main() -> i32 {
        strlen("hi") + strlen("bye")
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn libc_memset_and_free() {
    let source = source_raw!(
        r#"
    extern {
        fn malloc(size: i32) -> *;
        fn memset(ptr: *, value: i32, size: i32) -> *;
        fn free(ptr: *);
    }

    fn main() -> i32 {
        let ptr = malloc(32);
        memset(ptr, 0, 32);
        free(ptr);
        0
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 0);
}

#[test]
fn extern_in_function() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn my_abs(x: i32) -> i32 {
        abs(0 - x)
    }

    fn main() -> i32 {
        my_abs(7)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 7);
}

#[test]
fn extern_multiple_types() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
        fn strlen(s: &str) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 3) + strlen("ab")
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

#[test]
fn extern_with_let() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        let n = abs(0 - 42);
        n
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 42);
}

#[test]
fn extern_result_in_arithmetic() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(0 - 3) * abs(0 - 4)
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 12);
}

#[test]
fn extern_chained() {
    let source = source_raw!(
        r#"
    extern {
        fn abs(x: i32) -> i32;
    }

    fn main() -> i32 {
        abs(abs(0 - 5))
    }
    "#
    );
    let code = compile_and_run(source);
    assert_eq!(code, 5);
}

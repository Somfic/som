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

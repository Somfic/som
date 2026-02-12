mod common;

use common::compile_and_run;
use som::source_raw;

#[test]
fn identity() {
    let source = source_raw!(
        r#"
    extern "test_ffi.so" {
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

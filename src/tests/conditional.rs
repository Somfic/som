use super::run_and_assert;

#[test]
fn expression() {
    run_and_assert("fn main() 1 if true else 0", 1);
    run_and_assert("fn main() 1 if false else 0", 0);
}

#[test]
fn statement() {
    run_and_assert("fn main() { let a = 0; if true a = 1; a }", 1);
    run_and_assert("fn main() { let a = 0; if false a = 1; a }", 0);
}

#[test]
fn block() {
    run_and_assert("fn main() { let a = 0; if true { let a = 1; }; a }", 0);
    run_and_assert("fn main() { let a = 0; if false { let a = 1; }; a }", 0);
}

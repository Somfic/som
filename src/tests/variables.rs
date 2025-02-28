use super::run_and_assert;

#[test]
fn variables() {
    run_and_assert("fn main() { let a = 12; a }", 12);
    run_and_assert("fn main() { let a = 12; let b = a; b }", 12);
}

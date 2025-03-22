use super::run_and_assert;

#[test]
fn declaration() {
    run_and_assert("fn main() { let a = 12; a }", 12);
    run_and_assert("fn main() { let a = 12; let b = a; b }", 12);
}

#[test]
fn assignment() {
    run_and_assert("fn main() { let a = 12; a = 13; a }", 13);
    run_and_assert("fn main() { let a = 12; let b = a; b = 13; b }", 13);
}

use super::run_and_assert;

#[test]
fn conditional() {
    run_and_assert("fn main() 1 if true else 0", 1);
    run_and_assert("fn main() 1 if false else 0", 0);
}

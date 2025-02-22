use super::run_and_assert;

#[test]
fn unary() {
    run_and_assert("-1 + 2", 1);
    run_and_assert("1 - -2", 3);
}

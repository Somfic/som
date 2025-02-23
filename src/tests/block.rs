use super::run_and_assert;

#[test]
fn block() {
    run_and_assert("{ 1 + 1; 1 + 1 }", 2);
    run_and_assert("{ 1 + 1; 1 + 1; 0 }", 0);
}

use super::run_and_assert;

#[test]
fn variables() {
    run_and_assert("{ let a = 12; a }", 12);
}

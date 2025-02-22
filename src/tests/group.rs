use super::run_and_assert;

#[test]
fn group() {
    let source_code = "1 + 2 * (3 + 4)";
    let expected = 15;

    run_and_assert(source_code, expected);
}

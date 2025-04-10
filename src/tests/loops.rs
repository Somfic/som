use super::run_and_assert;

#[test]
fn while_loop() {
    let source_code = "
        fn main() {
            let i = 0;
            while i < 10 {
                i = i + 1;
            };
            i
        }
    ";
    let expected = 10;

    run_and_assert(source_code, expected);
}

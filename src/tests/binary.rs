use super::run_and_assert;

#[test]
fn addition() {
    run_and_assert("fn main() 1 + 1", 2);
    run_and_assert("fn main() 1 + 2 + 3", 6);
    run_and_assert("fn main() 1 + 2 + 3 + 4 + 5", 15);
}

#[test]
fn subtraction() {
    run_and_assert("fn main() 1 - 1", 0);
    run_and_assert("fn main() 1 - 2 - 3", -4);
    run_and_assert("fn main() 1 - 2 - 3 - 4 - 5", -13);
}

#[test]
fn multiplication() {
    run_and_assert("fn main() 1 * 1", 1);
    run_and_assert("fn main() 1 * 2 * 3", 6);
    run_and_assert("fn main() 1 * 2 * 3 * 4 * 5", 120);
}

#[test]
fn division() {
    run_and_assert("fn main() 1 / 1", 1);
    run_and_assert("fn main() 1 / 2 / 3", 0);
    run_and_assert("fn main() 1 / 2 / 3 / 4 / 5", 0);
}

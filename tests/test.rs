mod common;
use common::*;

#[test]
fn addition() {
    expect("1 + 1", 2);
}

#[test]
fn subtraction() {
    expect("2 - 1", 1);
}

#[test]
fn multiplication() {
    expect("2 * 3", 6);
}

#[test]
fn division() {
    expect("6 / 3", 2);
}

#[test]
fn operator_precedence() {
    expect("1 + 2 * 3", 7);
}

#[test]
fn parentheses() {
    expect("(1 + 2) * 3", 9);
}

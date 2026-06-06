mod common;
use common::*;

#[test]
fn addition() {
    expect("1 + 1", 2);
    expect("1 + 2 + 3", 6);
}

#[test]
fn subtraction() {
    expect("2 - 1", 1);
    expect("2 - 1 - 1", 0);
}

#[test]
fn multiplication() {
    expect("2 * 3", 6);
    expect("2 * 3 * 4", 24);
}

#[test]
fn division() {
    expect("6 / 3", 2);
    expect("128 / 2 / 2 / 2 / 2", 8);
}

#[test]
fn operator_precedence() {
    expect("1 + 2 * 3", 7);
    expect("1 * 2 + 3", 5);
}

#[test]
fn parentheses() {
    expect("(1 + 2) * 3", 9);
    expect("2 * (3 + 4)", 14);
    expect("2 * (3 + 4) * 5", 70);
}

#[test]
fn negation() {
    expect("-5", -5);
    expect("--5", 5);
}

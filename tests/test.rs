mod common;
use common::*;

#[test]
fn int_literal() {
    expect("42", 42);
}

#[test]
fn bool_literal() {
    expect("true", 1);
    expect("false", 0);
}

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

#[test]
fn not() {
    expect("!true", 0);
    expect("!false", 1);
    expect("!!true", 1);
    expect("!!false", 0);
}

#[test]
fn type_mismatch() {
    expect_type_error("true + 1");
    expect_type_error("1 + false");
    expect_type_error("-true");
    expect_type_error("!1");
}

#[test]
fn ordering_type_errors() {
    expect_type_error("true < false");
    expect_type_error("1 < true");
    expect_type_error("false >= 1");
}

#[test]
fn equality_type_errors() {
    expect_type_error("1 == true");
    expect_type_error("true != 1");
}

#[test]
fn logical_type_errors() {
    expect_type_error("1 && 2");
    expect_type_error("true && 1");
    expect_type_error("1 || false");
}

#[test]
fn equals() {
    expect("1 == 1", 1);
    expect("1 == 2", 0);
    expect("true == true", 1);
    expect("true == false", 0);
    expect("false == false", 1);
}

#[test]
fn not_equals() {
    expect("1 != 1", 0);
    expect("1 != 2", 1);
    expect("true != true", 0);
    expect("true != false", 1);
    expect("false != false", 0);
}

#[test]
fn less_than() {
    expect("1 < 2", 1);
    expect("2 < 1", 0);
    expect("1 < 1", 0);
}

#[test]
fn less_than_equals() {
    expect("1 <= 2", 1);
    expect("2 <= 1", 0);
    expect("1 <= 1", 1);
}

#[test]
fn greater_than() {
    expect("2 > 1", 1);
    expect("1 > 2", 0);
    expect("1 > 1", 0);
}

#[test]
fn greater_than_equals() {
    expect("2 >= 1", 1);
    expect("1 >= 2", 0);
    expect("1 >= 1", 1);
}

#[test]
fn and() {
    expect("true && true", 1);
    expect("true && false", 0);
    expect("false && true", 0);
    expect("false && false", 0);
}

#[test]
fn or() {
    expect("true || true", 1);
    expect("true || false", 1);
    expect("false || true", 1);
    expect("false || false", 0);
}

#[test]
fn comparison_precedence() {
    expect("1 + 2 < 4", 1);
    expect("2 * 3 > 5", 1);
    expect("1 < 2 == true", 1);
    expect("3 > 4 == false", 1);
}

#[test]
fn logical_precedence() {
    expect("true || false && false", 1);
    expect("true && 1 == 1", 1);
    expect("1 < 2 && 3 > 2 || false", 1);
}

use crate::tests::interpret;

#[test]
fn less_than_basic() {
    assert_eq!(1, interpret("1 < 2"));
    assert_eq!(0, interpret("2 < 1"));
    assert_eq!(0, interpret("1 < 1"));
}

#[test]
fn less_than_i32() {
    assert_eq!(1, interpret("1i < 2i"));
    assert_eq!(0, interpret("2i < 1i"));
    assert_eq!(0, interpret("1i < 1i"));
}

#[test]
fn less_than_i64() {
    assert_eq!(1, interpret("1l < 2l"));
    assert_eq!(0, interpret("2l < 1l"));
    assert_eq!(0, interpret("1l < 1l"));
}

#[test]
fn less_than_negative() {
    assert_eq!(1, interpret("-2 < -1"));
    assert_eq!(1, interpret("-1 < 0"));
    assert_eq!(1, interpret("-1 < 1"));
    assert_eq!(0, interpret("-1 < -2"));
}

#[test]
fn less_than_zero() {
    assert_eq!(1, interpret("0 < 1"));
    assert_eq!(0, interpret("1 < 0"));
    assert_eq!(0, interpret("0 < 0"));
}

#[test]
fn less_than_larger_numbers() {
    assert_eq!(1, interpret("100 < 200"));
    assert_eq!(0, interpret("200 < 100"));
    assert_eq!(1, interpret("999 < 1000"));
}

#[test]
fn less_than_with_expressions() {
    assert_eq!(1, interpret("1 + 1 < 3"));
    assert_eq!(0, interpret("2 + 2 < 3"));
    assert_eq!(1, interpret("1 < 2 + 1"));
    assert_eq!(0, interpret("5 < 2 + 1"));
}

#[test]
fn less_than_grouping() {
    assert_eq!(1, interpret("(1) < (2)"));
    assert_eq!(0, interpret("(2) < (1)"));
    assert_eq!(1, interpret("(1 + 1) < (2 + 1)"));
    assert_eq!(0, interpret("(2 + 1) < (1 + 1)"));
}

#[test]
fn less_than_precedence() {
    assert_eq!(1, interpret("1 + 2 < 2 + 3"));
    assert_eq!(0, interpret("2 + 3 < 1 + 2"));
    assert_eq!(1, interpret("2 * 3 < 3 * 3"));
    assert_eq!(0, interpret("3 * 3 < 2 * 3"));
}
use crate::tests::interpret;

#[test]
fn negative() {
    assert_eq!(-1, interpret("-1"));
    assert_eq!(-1, interpret("-1i32"));
    assert_eq!(-1, interpret("-1i64"));
    assert_eq!(1, interpret("-(-1)"));
}

#[test]
fn addition() {
    assert_eq!(2, interpret("1 + 1"));
    assert_eq!(2, interpret("1i32 + 1i32"));
    assert_eq!(2, interpret("1i64 + 1i64"));
    assert_eq!(10, interpret("1 + 2 + 3 + 4"));
}

#[test]
fn subtraction() {
    assert_eq!(0, interpret("1 - 1"));
    assert_eq!(0, interpret("1i32 - 1i32"));
    assert_eq!(0, interpret("1i64 - 1i64"));
    assert_eq!(1, interpret("2 - 1"));
    assert_eq!(-1, interpret("1 - 2"));
    assert_eq!(100, interpret("202 - 101"));
}

#[test]
fn multiplication() {
    assert_eq!(1, interpret("1 * 1"));
    assert_eq!(1, interpret("1i32 * 1i32"));
    assert_eq!(1, interpret("1i64 * 1i64"));
    assert_eq!(2, interpret("1 * 2"));
    assert_eq!(2, interpret("2 * 1"));
    assert_eq!(6, interpret("2 * 3"));
    assert_eq!(100, interpret("10 * 10"));
}

#[test]
fn division() {
    assert_eq!(1, interpret("1 / 1"));
    assert_eq!(1, interpret("1i32 / 1i32"));
    assert_eq!(1, interpret("1i64 / 1i64"));
    assert_eq!(2, interpret("4 / 2"));
    assert_eq!(2, interpret("2 / 1"));
    assert_eq!(3, interpret("6 / 2"));
    assert_eq!(10, interpret("100 / 10"));
}

#[test]
fn grouping() {
    assert_eq!(2, interpret("(1 + 1)"));
    assert_eq!(2, interpret("1 + (1)"));
    assert_eq!(2, interpret("(1 + 1) + 0"));
    assert_eq!(2, interpret("0 + (1 + 1)"));
    assert_eq!(4, interpret("(2 * 2)"));
    assert_eq!(4, interpret("2 * (2)"));
    assert_eq!(4, interpret("(2 * 2) + 0"));
    assert_eq!(4, interpret("0 + (2 * 2)"));
    assert_eq!(6, interpret("(2 + 2) * 3"));
    assert_eq!(6, interpret("2 * (2 + 3)"));
    assert_eq!(6, interpret("(2 + 3) * 2"));
    assert_eq!(15, interpret("(2 + 3) * (4 - 1)"));
}

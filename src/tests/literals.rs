use crate::tests::interpret;

#[test]
fn int_32() {
    assert_eq!(1, interpret("1"));
    assert_eq!(1, interpret("1i32"));
    assert_eq!(-1, interpret("-1"));
    assert_eq!(-1, interpret("-1i32"));
    assert_eq!(2000, interpret("2000"));
    assert_eq!(2000, interpret("2000i32"));
}

#[test]
fn int_64() {
    assert_eq!(1, interpret("1i64"));
    assert_eq!(-1, interpret("-1i64"));
    assert_eq!(2000, interpret("2000i64"));
    assert_eq!(2000, interpret("2000i64"));
}

#[test]
fn boolean() {
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));
}

// TODO: hexadecimal, octal, binary literals

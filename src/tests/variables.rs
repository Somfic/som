use crate::tests::interpret;

#[test]
fn declaration() {
    assert_eq!(5, interpret("let a = 5; a"));
    assert_eq!(1, interpret("let a = true; a"));
    assert_eq!(0, interpret("let a = false; a"));
    assert_eq!(1, interpret("let a = 1i; a"));
    assert_eq!(1, interpret("let a = 1l; a"));
    assert_eq!(2, interpret("let a = 1; a + 1"));
    assert_eq!(3, interpret("let a = 1; let b = 2; a + b"));
    assert_eq!(6, interpret("let a = 1; let b = 2; let c = 3; a + b + c"));
}

#[test]
fn assignment() {
    assert_eq!(5, interpret("let a = 1; a = 5; a"));
    assert_eq!(10, interpret("let a = 1; a = 5; a + 5"));
    assert_eq!(3, interpret("let a = 1; let b = 1; b = 2; a + b"));
    assert_eq!(4, interpret("let a = 1; let b = 2; b = 3; a + b"));
}

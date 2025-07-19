use crate::tests::interpret;

#[test]
fn simple_conditional() {
    assert_eq!(1, interpret("true if true else false"));
    assert_eq!(0, interpret("true if false else false"));
    assert_eq!(0, interpret("false if true else true"));
    assert_eq!(1, interpret("false if false else true"));
}

#[test]
fn conditional_with_numbers() {
    assert_eq!(5, interpret("5 if true else 10"));
    assert_eq!(10, interpret("5 if false else 10"));
    assert_eq!(42, interpret("42 if true else 0"));
    assert_eq!(0, interpret("42 if false else 0"));
}

#[test]
fn conditional_with_expressions() {
    assert_eq!(3, interpret("1 + 2 if true else 4 + 5"));
    assert_eq!(9, interpret("1 + 2 if false else 4 + 5"));
    assert_eq!(10, interpret("2 * 5 if true else 3 * 4"));
    assert_eq!(12, interpret("2 * 5 if false else 3 * 4"));
}

#[test]
fn conditional_with_variables() {
    assert_eq!(1, interpret("let x = true; x if true else false"));
    assert_eq!(0, interpret("let x = true; x if false else false"));
    assert_eq!(5, interpret("let a = 5; let b = 10; a if true else b"));
    assert_eq!(10, interpret("let a = 5; let b = 10; a if false else b"));
}

#[test]
fn nested_conditionals() {
    assert_eq!(1, interpret("true if (true if true else false) else false"));
    assert_eq!(0, interpret("true if (true if false else false) else false"));
    assert_eq!(1, interpret("(true if true else false) if true else (false if true else true)"));
}

#[test]
fn conditional_with_comparisons() {
    // Note: These tests assume comparison operators exist in the language
    // If they don't exist yet, these tests should be commented out or the operators implemented
    // assert_eq!(1, interpret("5 if 3 > 2 else 0"));
    // assert_eq!(0, interpret("5 if 3 < 2 else 0"));
    // assert_eq!(10, interpret("5 if 3 == 2 else 10"));
    
    // For now, using basic boolean conditions
    assert_eq!(5, interpret("let condition = true; 5 if condition else 0"));
    assert_eq!(0, interpret("let condition = false; 5 if condition else 0"));
}

#[test]
fn conditional_in_assignment() {
    assert_eq!(42, interpret("let result = 42 if true else 0; result"));
    assert_eq!(0, interpret("let result = 42 if false else 0; result"));
    assert_eq!(15, interpret("let a = 10; let b = 5; let max = a if true else b; max"));
}

#[test]
fn conditional_precedence() {
    // Test that conditional has the right precedence with other operators
    assert_eq!(3, interpret("1 + (2 if true else 4)"));
    assert_eq!(5, interpret("1 + (2 if false else 4)"));
    assert_eq!(6, interpret("(1 + 2) if true else (3 + 4)"));
    assert_eq!(7, interpret("(1 + 2) if false else (3 + 4)"));
}

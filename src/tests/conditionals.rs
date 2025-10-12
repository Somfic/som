use crate::tests::interpret;

#[test]
fn basic_conditional_true() {
    assert_eq!(1, interpret("1 if true else 0"));
    assert_eq!(5, interpret("5 if true else 10"));
    assert_eq!(100, interpret("100 if true else 0"));
}

#[test]
fn basic_conditional_false() {
    assert_eq!(0, interpret("1 if false else 0"));
    assert_eq!(10, interpret("5 if false else 10"));
    assert_eq!(0, interpret("100 if false else 0"));
}

#[test]
fn conditional_with_expressions() {
    assert_eq!(7, interpret("2 + 5 if true else 0"));
    assert_eq!(0, interpret("2 + 5 if false else 0"));
    assert_eq!(10, interpret("1 + 1 if false else 5 * 2"));
}

#[test]
fn conditional_with_variables() {
    assert_eq!(5, interpret("let x = 5; x if true else 0"));
    assert_eq!(10, interpret("let x = 5; let y = 10; x if false else y"));
    assert_eq!(15, interpret("let a = 15; let b = 20; a if true else b"));
}

#[test]
fn conditional_with_arithmetic_in_branches() {
    assert_eq!(10, interpret("5 * 2 if true else 5 + 2"));
    assert_eq!(7, interpret("5 * 2 if false else 5 + 2"));
    assert_eq!(20, interpret("10 + 10 if true else 10 - 10"));
}

#[test]
fn conditional_with_comparisons() {
    assert_eq!(1, interpret("1 if 5 < 10 else 0"));
    assert_eq!(0, interpret("1 if 10 < 5 else 0"));
    assert_eq!(100, interpret("100 if 1 < 2 else 50"));
}

#[test]
fn conditional_returning_booleans() {
    assert_eq!(1, interpret("true if true else false"));
    assert_eq!(0, interpret("false if true else true"));
    assert_eq!(1, interpret("true if false else true"));
    assert_eq!(0, interpret("false if false else false"));
}

#[test]
fn nested_conditionals() {
    assert_eq!(1, interpret("1 if true else (2 if true else 3)"));
    assert_eq!(2, interpret("1 if false else (2 if true else 3)"));
    assert_eq!(3, interpret("1 if false else (2 if false else 3)"));
}

#[test]
fn conditional_with_grouping() {
    assert_eq!(10, interpret("(5 + 5) if true else (2 * 2)"));
    assert_eq!(4, interpret("(5 + 5) if false else (2 * 2)"));
    assert_eq!(20, interpret("((2 + 3) * 4) if true else 0"));
}

#[test]
fn conditional_in_variable_declaration() {
    assert_eq!(1, interpret("let result = 1 if true else 0; result"));
    assert_eq!(10, interpret("let value = 5 if false else 10; value"));
    assert_eq!(
        15,
        interpret("let x = 10; let y = 20; let result = x if true else y; result + 5")
    );
}

#[test]
fn conditional_in_function_call() {
    assert_eq!(
        10,
        interpret("let double = fn(x ~ int) -> int { x * 2 }; double(5 if true else 0)")
    );
    assert_eq!(
        0,
        interpret("let double = fn(x ~ int) -> int { x * 2 }; double(5 if false else 0)")
    );
}

#[test]
fn conditional_with_function_calls() {
    assert_eq!(
        10,
        interpret("let get5 = fn() -> int { 5 }; let get10 = fn() -> int { 10 }; get5() if false else get10()")
    );
    assert_eq!(
        5,
        interpret("let get5 = fn() -> int { 5 }; let get10 = fn() -> int { 10 }; get5() if true else get10()")
    );
}

#[test]
fn conditional_with_blocks() {
    assert_eq!(5, interpret("{ 5 } if true else { 10 }"));
    assert_eq!(10, interpret("{ 5 } if false else { 10 }"));
    assert_eq!(
        15,
        interpret("{ let x = 5; x + 10 } if true else { 0 }")
    );
}

#[test]
fn conditional_complex_conditions() {
    assert_eq!(1, interpret("let x = 5; 1 if x < 10 else 0"));
    assert_eq!(0, interpret("let x = 15; 1 if x < 10 else 0"));
    assert_eq!(
        100,
        interpret("let a = 3; let b = 7; 100 if a < b else 50")
    );
}

#[test]
fn conditional_both_branches_same_value() {
    assert_eq!(5, interpret("5 if true else 5"));
    assert_eq!(5, interpret("5 if false else 5"));
    assert_eq!(0, interpret("0 if true else 0"));
}

#[test]
fn conditional_with_negative_numbers() {
    assert_eq!(-5, interpret("-5 if true else 5"));
    assert_eq!(5, interpret("-5 if false else 5"));
    assert_eq!(-10, interpret("10 if false else -10"));
}

#[test]
fn conditional_precedence_with_operators() {
    // Test that conditional has lower precedence than arithmetic
    assert_eq!(3, interpret("1 + 2 if true else 3 * 4")); // (1 + 2) if true else (3 * 4)
    assert_eq!(12, interpret("1 + 2 if false else 3 * 4")); // (1 + 2) if false else (3 * 4)
}

#[test]
fn conditional_chaining() {
    // Test multiple conditionals in sequence
    assert_eq!(
        1,
        interpret("let flag1 = true; let flag2 = false; 1 if flag1 else (2 if flag2 else 3)")
    );
    assert_eq!(
        3,
        interpret("let flag1 = false; let flag2 = false; 1 if flag1 else (2 if flag2 else 3)")
    );
}

#[test]
fn conditional_with_zero_values() {
    assert_eq!(0, interpret("0 if true else 1"));
    assert_eq!(1, interpret("0 if false else 1"));
    assert_eq!(0, interpret("1 if false else 0"));
}

#[test]
fn conditional_in_complex_expression() {
    assert_eq!(
        20,
        interpret("let x = 5; (x * 2 if x < 10 else x * 4) + 10")
    );
    assert_eq!(
        70,
        interpret("let x = 15; (x * 2 if x < 10 else x * 4) + 10")
    );
}

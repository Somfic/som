use crate::tests::interpret;

// Less than (<) tests
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

// Greater than (>) tests
#[test]
fn greater_than_basic() {
    assert_eq!(0, interpret("1 > 2"));
    assert_eq!(1, interpret("2 > 1"));
    assert_eq!(0, interpret("1 > 1"));
}

#[test]
fn greater_than_i32() {
    assert_eq!(0, interpret("1i > 2i"));
    assert_eq!(1, interpret("2i > 1i"));
    assert_eq!(0, interpret("1i > 1i"));
}

#[test]
fn greater_than_i64() {
    assert_eq!(0, interpret("1l > 2l"));
    assert_eq!(1, interpret("2l > 1l"));
    assert_eq!(0, interpret("1l > 1l"));
}

#[test]
fn greater_than_negative() {
    assert_eq!(0, interpret("-2 > -1"));
    assert_eq!(0, interpret("-1 > 0"));
    assert_eq!(0, interpret("-1 > 1"));
    assert_eq!(1, interpret("-1 > -2"));
}

#[test]
fn greater_than_zero() {
    assert_eq!(0, interpret("0 > 1"));
    assert_eq!(1, interpret("1 > 0"));
    assert_eq!(0, interpret("0 > 0"));
}

#[test]
fn greater_than_larger_numbers() {
    assert_eq!(0, interpret("100 > 200"));
    assert_eq!(1, interpret("200 > 100"));
    assert_eq!(0, interpret("999 > 1000"));
    assert_eq!(1, interpret("1000 > 999"));
}

#[test]
fn greater_than_with_expressions() {
    assert_eq!(0, interpret("1 + 1 > 3"));
    assert_eq!(1, interpret("2 + 2 > 3"));
    assert_eq!(0, interpret("5 > 2 + 4"));
    assert_eq!(1, interpret("5 > 2 + 1"));
}

#[test]
fn greater_than_precedence() {
    assert_eq!(0, interpret("1 + 2 > 2 + 3"));
    assert_eq!(1, interpret("2 + 3 > 1 + 2"));
    assert_eq!(0, interpret("2 * 3 > 3 * 3"));
    assert_eq!(1, interpret("3 * 3 > 2 * 3"));
}

// Less than or equal (<=) tests
#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_basic() {
    assert_eq!(1, interpret("1 <= 2"));
    assert_eq!(0, interpret("2 <= 1"));
    assert_eq!(1, interpret("1 <= 1"));
}

#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_i32() {
    assert_eq!(1, interpret("1i <= 2i"));
    assert_eq!(0, interpret("2i <= 1i"));
    assert_eq!(1, interpret("1i <= 1i"));
}

#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_i64() {
    assert_eq!(1, interpret("1l <= 2l"));
    assert_eq!(0, interpret("2l <= 1l"));
    assert_eq!(1, interpret("1l <= 1l"));
}

#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_negative() {
    assert_eq!(1, interpret("-2 <= -1"));
    assert_eq!(1, interpret("-1 <= 0"));
    assert_eq!(1, interpret("-1 <= 1"));
    assert_eq!(0, interpret("-1 <= -2"));
    assert_eq!(1, interpret("-5 <= -5"));
}

#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_zero() {
    assert_eq!(1, interpret("0 <= 1"));
    assert_eq!(0, interpret("1 <= 0"));
    assert_eq!(1, interpret("0 <= 0"));
}

#[test]
#[ignore = "Less than or equal (<=) operator is not yet implemented"]
fn less_than_or_equal_with_expressions() {
    assert_eq!(1, interpret("1 + 1 <= 3"));
    assert_eq!(1, interpret("2 + 2 <= 4"));
    assert_eq!(0, interpret("2 + 2 <= 3"));
}

// Greater than or equal (>=) tests
#[test]
fn greater_than_or_equal_basic() {
    assert_eq!(0, interpret("1 >= 2"));
    assert_eq!(1, interpret("2 >= 1"));
    assert_eq!(1, interpret("1 >= 1"));
}

#[test]
fn greater_than_or_equal_i32() {
    assert_eq!(0, interpret("1i >= 2i"));
    assert_eq!(1, interpret("2i >= 1i"));
    assert_eq!(1, interpret("1i >= 1i"));
}

#[test]
fn greater_than_or_equal_i64() {
    assert_eq!(0, interpret("1l >= 2l"));
    assert_eq!(1, interpret("2l >= 1l"));
    assert_eq!(1, interpret("1l >= 1l"));
}

#[test]
fn greater_than_or_equal_negative() {
    assert_eq!(0, interpret("-2 >= -1"));
    assert_eq!(0, interpret("-1 >= 0"));
    assert_eq!(0, interpret("-1 >= 1"));
    assert_eq!(1, interpret("-1 >= -2"));
    assert_eq!(1, interpret("-5 >= -5"));
}

#[test]
fn greater_than_or_equal_zero() {
    assert_eq!(0, interpret("0 >= 1"));
    assert_eq!(1, interpret("1 >= 0"));
    assert_eq!(1, interpret("0 >= 0"));
}

#[test]
fn greater_than_or_equal_with_expressions() {
    assert_eq!(0, interpret("1 + 1 >= 3"));
    assert_eq!(1, interpret("2 + 2 >= 4"));
    assert_eq!(1, interpret("2 + 3 >= 4"));
}

// Equality (==) tests
#[test]
fn equality_basic() {
    assert_eq!(0, interpret("1 == 2"));
    assert_eq!(1, interpret("1 == 1"));
    assert_eq!(1, interpret("5 == 5"));
    assert_eq!(0, interpret("5 == 10"));
}

#[test]
fn equality_i32() {
    assert_eq!(1, interpret("1i == 1i"));
    assert_eq!(0, interpret("1i == 2i"));
    assert_eq!(1, interpret("100i == 100i"));
}

#[test]
fn equality_i64() {
    assert_eq!(1, interpret("1l == 1l"));
    assert_eq!(0, interpret("1l == 2l"));
    assert_eq!(1, interpret("100l == 100l"));
}

#[test]
fn equality_negative() {
    assert_eq!(1, interpret("-1 == -1"));
    assert_eq!(0, interpret("-1 == 1"));
    assert_eq!(1, interpret("-100 == -100"));
    assert_eq!(0, interpret("-5 == -10"));
}

#[test]
fn equality_zero() {
    assert_eq!(1, interpret("0 == 0"));
    assert_eq!(0, interpret("0 == 1"));
    assert_eq!(0, interpret("1 == 0"));
}

#[test]
#[ignore = "Boolean equality comparison is not yet supported by the type checker"]
fn equality_booleans() {
    assert_eq!(1, interpret("true == true"));
    assert_eq!(1, interpret("false == false"));
    assert_eq!(0, interpret("true == false"));
    assert_eq!(0, interpret("false == true"));
}

#[test]
fn equality_with_expressions() {
    assert_eq!(1, interpret("1 + 1 == 2"));
    assert_eq!(1, interpret("2 * 3 == 6"));
    assert_eq!(0, interpret("1 + 1 == 3"));
    assert_eq!(1, interpret("(2 + 3) == (1 + 4)"));
}

// Inequality (!=) tests
#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_basic() {
    assert_eq!(1, interpret("1 != 2"));
    assert_eq!(0, interpret("1 != 1"));
    assert_eq!(0, interpret("5 != 5"));
    assert_eq!(1, interpret("5 != 10"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_i32() {
    assert_eq!(0, interpret("1i != 1i"));
    assert_eq!(1, interpret("1i != 2i"));
    assert_eq!(0, interpret("100i != 100i"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_i64() {
    assert_eq!(0, interpret("1l != 1l"));
    assert_eq!(1, interpret("1l != 2l"));
    assert_eq!(0, interpret("100l != 100l"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_negative() {
    assert_eq!(0, interpret("-1 != -1"));
    assert_eq!(1, interpret("-1 != 1"));
    assert_eq!(0, interpret("-100 != -100"));
    assert_eq!(1, interpret("-5 != -10"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_zero() {
    assert_eq!(0, interpret("0 != 0"));
    assert_eq!(1, interpret("0 != 1"));
    assert_eq!(1, interpret("1 != 0"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_booleans() {
    assert_eq!(0, interpret("true != true"));
    assert_eq!(0, interpret("false != false"));
    assert_eq!(1, interpret("true != false"));
    assert_eq!(1, interpret("false != true"));
}

#[test]
#[ignore = "Not equal (!=) operator is not yet implemented"]
fn inequality_with_expressions() {
    assert_eq!(0, interpret("1 + 1 != 2"));
    assert_eq!(0, interpret("2 * 3 != 6"));
    assert_eq!(1, interpret("1 + 1 != 3"));
    assert_eq!(0, interpret("(2 + 3) != (1 + 4)"));
}

// Mixed comparison tests
#[test]
fn comparison_chaining_in_conditionals() {
    assert_eq!(1, interpret("1 if 1 < 2 else 0"));
    assert_eq!(0, interpret("1 if 1 > 2 else 0"));
    assert_eq!(1, interpret("1 if 5 == 5 else 0"));
}

#[test]
fn comparison_with_variables() {
    // Note: Comparisons return i64 (1 or 0), not bool
    assert_eq!(
        1,
        interpret("let x = 5; let y = 10; let result = x < y; result")
    );
    assert_eq!(
        0,
        interpret("let x = 10; let y = 5; let result = x < y; result")
    );
    assert_eq!(
        1,
        interpret("let a = 5; let b = 5; let result = a == b; result")
    );
}

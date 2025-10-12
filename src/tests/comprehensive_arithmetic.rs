use crate::tests::interpret;

#[test]
fn test_comprehensive_addition() {
    // Basic addition
    assert_eq!(2, interpret("1 + 1"));
    assert_eq!(10, interpret("3 + 7"));
    assert_eq!(0, interpret("5 + (-5)"));

    // Addition with different number formats
    assert_eq!(2, interpret("1i + 1i"));
    assert_eq!(2, interpret("1l + 1l"));

    // Chain addition
    assert_eq!(6, interpret("1 + 2 + 3"));
    assert_eq!(10, interpret("1 + 2 + 3 + 4"));
    assert_eq!(15, interpret("1 + 2 + 3 + 4 + 5"));

    // Addition with variables
    assert_eq!(8, interpret("let a = 3; let b = 5; a + b"));
    assert_eq!(12, interpret("let x = 4; x + x + x"));

    // Addition in expressions
    assert_eq!(14, interpret("(3 + 4) + 7"));
    assert_eq!(14, interpret("3 + (4 + 7)"));
    assert_eq!(20, interpret("(2 + 3) + (5 + 10)"));
}

#[test]
fn test_comprehensive_subtraction() {
    // Basic subtraction
    assert_eq!(0, interpret("1 - 1"));
    assert_eq!(2, interpret("5 - 3"));
    assert_eq!(-2, interpret("3 - 5"));

    // Subtraction with negatives
    assert_eq!(8, interpret("3 - (-5)"));
    assert_eq!(-8, interpret("-3 - 5"));
    assert_eq!(2, interpret("-3 - (-5)"));

    // Chain subtraction (left associative)
    assert_eq!(-4, interpret("1 - 2 - 3"));
    assert_eq!(-8, interpret("1 - 2 - 3 - 4"));

    // Subtraction with variables
    assert_eq!(2, interpret("let a = 7; let b = 5; a - b"));
    assert_eq!(0, interpret("let x = 4; x - x"));

    // Mixed addition and subtraction
    assert_eq!(8, interpret("10 - 3 + 2 - 1"));
    assert_eq!(4, interpret("1 + 5 - 2"));
}

#[test]
fn test_comprehensive_multiplication() {
    // Basic multiplication
    assert_eq!(6, interpret("2 * 3"));
    assert_eq!(20, interpret("4 * 5"));
    assert_eq!(0, interpret("0 * 100"));
    assert_eq!(0, interpret("100 * 0"));

    // Multiplication with negatives
    assert_eq!(-6, interpret("2 * (-3)"));
    assert_eq!(-6, interpret("(-2) * 3"));
    assert_eq!(6, interpret("(-2) * (-3)"));

    // Chain multiplication
    assert_eq!(24, interpret("2 * 3 * 4"));
    assert_eq!(120, interpret("2 * 3 * 4 * 5"));

    // Multiplication with variables
    assert_eq!(35, interpret("let a = 5; let b = 7; a * b"));
    assert_eq!(16, interpret("let x = 4; x * x"));

    // Multiplication precedence
    assert_eq!(14, interpret("2 + 3 * 4")); // 2 + 12 = 14
    assert_eq!(10, interpret("2 * 3 + 4")); // 6 + 4 = 10
    assert_eq!(26, interpret("2 + 3 * 4 * 2")); // 2 + 24 = 26
}

#[test]
fn test_comprehensive_division() {
    // Basic division
    assert_eq!(2, interpret("6 / 3"));
    assert_eq!(5, interpret("25 / 5"));
    assert_eq!(1, interpret("7 / 7"));

    // Integer division (truncation)
    assert_eq!(2, interpret("7 / 3"));
    assert_eq!(3, interpret("10 / 3"));
    assert_eq!(0, interpret("3 / 10"));

    // Division with negatives
    assert_eq!(-2, interpret("6 / (-3)"));
    assert_eq!(-2, interpret("(-6) / 3"));
    assert_eq!(2, interpret("(-6) / (-3)"));

    // Chain division (left associative)
    assert_eq!(1, interpret("12 / 3 / 4")); // (12 / 3) / 4 = 4 / 4 = 1
    assert_eq!(2, interpret("24 / 3 / 4")); // (24 / 3) / 4 = 8 / 4 = 2

    // Division with variables
    assert_eq!(4, interpret("let a = 20; let b = 5; a / b"));
    assert_eq!(1, interpret("let x = 7; x / x"));

    // Division precedence
    assert_eq!(6, interpret("4 + 8 / 4")); // 4 + 2 = 6
    assert_eq!(3, interpret("(4 + 8) / 4")); // 12 / 4 = 3
    assert_eq!(2, interpret("8 / 4 + 0")); // 2 + 0 = 2
}

#[test]
fn test_unary_operations() {
    // Basic unary minus
    assert_eq!(-5, interpret("-5"));
    assert_eq!(-1, interpret("-1"));
    assert_eq!(0, interpret("-0"));

    // Double negative
    assert_eq!(5, interpret("-(-5)"));
    assert_eq!(1, interpret("-(-1)"));

    // Unary with variables
    assert_eq!(-7, interpret("let x = 7; -x"));
    assert_eq!(7, interpret("let y = -7; -y"));

    // Unary in expressions
    assert_eq!(-2, interpret("-(1 + 1)"));
    assert_eq!(-10, interpret("-(2 * 5)"));
    assert_eq!(3, interpret("-(-1 - 2)"));

    // Unary precedence with binary operations
    assert_eq!(-4, interpret("2 * -2")); // 2 * (-2) = -4
    assert_eq!(4, interpret("(-2) * (-2)")); // (-2) * (-2) = 4
    assert_eq!(-1, interpret("3 + -4")); // 3 + (-4) = -1
    assert_eq!(7, interpret("3 - -4")); // 3 - (-4) = 7
}

#[test]
fn test_complex_arithmetic_expressions() {
    // Complex precedence scenarios
    assert_eq!(21, interpret("1 + 2 * 3 + 4 * 5 - 6")); // 1 + 6 + 20 - 6 = 21
    assert_eq!(42, interpret("(1 + 2) * (3 + 4) * 2")); // 3 * 7 * 2 = 42
    assert_eq!(4, interpret("20 / (2 + 3)")); // 20 / 5 = 4
    assert_eq!(7, interpret("15 / 3 + 2")); // 5 + 2 = 7

    // Deeply nested expressions
    assert_eq!(32, interpret("((2 + 2) * (3 + 1)) * (5 - 3)")); // (4 * 4) * 2 = 16 * 2 = 32
    assert_eq!(100, interpret("(5 * (4 - 2)) * (8 + 2)")); // (5 * 2) * 10 = 100

    // Mixed operations with variables
    assert_eq!(19, interpret("let a = 3; let b = 4; a * b + 2 * a + 1")); // 12 + 6 + 1 = 19
    assert_eq!(65, interpret("let x = 5; let y = 10; x * y + x * x - y")); // 50 + 25 - 10 = 65
}

#[test]
fn test_arithmetic_with_all_operators() {
    // Test all four operators in single expressions
    assert_eq!(11, interpret("2 + 3 * 4 - 6 / 2")); // 2 + 12 - 3 = 11
    assert_eq!(4, interpret("20 / 5 + 8 / 4 - 2 * 1")); // 4 + 2 - 2 = 4
    assert_eq!(30, interpret("5 * 5 + 10 / 2 - 3 + 3")); // 25 + 5 - 3 + 3 = 30

    // Expressions with all operators and parentheses
    assert_eq!(18, interpret("(2 + 4) * 3 + 6 / 2 - 3")); // 6 * 3 + 3 - 3 = 18
    assert_eq!(12, interpret("2 * (3 + 4) + 8 / 4 - 4")); // 2 * 7 + 2 - 4 = 14 + 2 - 4 = 12
}

#[test]
fn test_zero_and_identity_operations() {
    // Addition with zero
    assert_eq!(5, interpret("5 + 0"));
    assert_eq!(5, interpret("0 + 5"));
    assert_eq!(0, interpret("0 + 0"));

    // Multiplication with zero
    assert_eq!(0, interpret("5 * 0"));
    assert_eq!(0, interpret("0 * 5"));
    assert_eq!(0, interpret("0 * 0"));

    // Multiplication with one
    assert_eq!(5, interpret("5 * 1"));
    assert_eq!(5, interpret("1 * 5"));
    assert_eq!(1, interpret("1 * 1"));

    // Division by one
    assert_eq!(5, interpret("5 / 1"));
    assert_eq!(1, interpret("1 / 1"));

    // Zero division (handled by error tests)
}

#[test]
fn test_associativity() {
    // Left associativity for same precedence operators
    assert_eq!(1, interpret("5 - 2 - 2")); // (5 - 2) - 2 = 1
    assert_eq!(1, interpret("8 / 4 / 2")); // (8 / 4) / 2 = 1
    assert_eq!(16, interpret("2 * 2 * 4")); // (2 * 2) * 4 = 16
    assert_eq!(6, interpret("1 + 2 + 3")); // (1 + 2) + 3 = 6

    // Test that operations of different precedence work correctly
    assert_eq!(14, interpret("2 + 3 * 4")); // 2 + (3 * 4) = 14
    assert_eq!(11, interpret("2 * 3 + 5")); // (2 * 3) + 5 = 11
    assert_eq!(7, interpret("15 / 3 + 2")); // (15 / 3) + 2 = 7
    assert_eq!(1, interpret("7 - 12 / 2")); // 7 - (12 / 2) = 1
}

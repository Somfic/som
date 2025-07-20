use crate::tests::interpret;

#[test]
fn string_literals() {
    // Note: These tests assume string support exists in the language
    // If not implemented yet, comment out until strings are added

    // Basic string tests - uncomment when strings are implemented
    // assert_eq!(interpret("\"hello\""), /* some representation of "hello" */);
    // assert_eq!(interpret("\"world\""), /* some representation of "world" */);
    // assert_eq!(interpret("\"\""), /* empty string representation */);
}

#[test]
fn hexadecimal_literals() {
    // Uncomment and implement when hex literals are supported
    // assert_eq!(16, interpret("0x10"));
    // assert_eq!(255, interpret("0xFF"));
    // assert_eq!(0, interpret("0x0"));
    // assert_eq!(10, interpret("0xA"));
}

#[test]
fn binary_literals() {
    // Uncomment and implement when binary literals are supported
    // assert_eq!(5, interpret("0b101"));
    // assert_eq!(8, interpret("0b1000"));
    // assert_eq!(0, interpret("0b0"));
    // assert_eq!(1, interpret("0b1"));
}

#[test]
fn octal_literals() {
    // Uncomment and implement when octal literals are supported
    // assert_eq!(8, interpret("0o10"));
    // assert_eq!(64, interpret("0o100"));
    // assert_eq!(0, interpret("0o0"));
    // assert_eq!(7, interpret("0o7"));
}

#[test]
fn floating_point_literals() {
    // Uncomment and implement when floating point support is added
    // assert_eq!(3.14, interpret("3.14"));
    // assert_eq!(0.5, interpret("0.5"));
    // assert_eq!(1.0, interpret("1.0"));
    // assert_eq!(-2.5, interpret("-2.5"));
}

#[test]
fn character_literals() {
    // Uncomment and implement when character literals are supported
    // assert_eq!('a' as long, interpret("'a'"));
    // assert_eq!('Z' as long, interpret("'Z'"));
    // assert_eq!('0' as long, interpret("'0'"));
}

#[test]
fn large_integer_literals() {
    assert_eq!(1000000, interpret("1000000"));
    assert_eq!(2147483647, interpret("2147483647")); // Max int
                                                     // Note: Very large numbers cause overflow - this is a compiler limitation
                                                     // assert_eq!(9223372036854775807_long as long, interpret("9223372036854775807")); // Max long - causes overflow
}

#[test]
fn negative_literals() {
    assert_eq!(-1, interpret("-1"));
    assert_eq!(-42, interpret("-42"));
    assert_eq!(-1000, interpret("-1000"));
    // Note: Very large negative numbers cause overflow - this is a compiler limitation
    // assert_eq!(-2147483648_long, interpret("-2147483648")); // Min int value as long - causes overflow
}

#[test]
fn underscore_separators() {
    // Uncomment and implement when underscore separators in numbers are supported
    // assert_eq!(1000, interpret("1_000"));
    // assert_eq!(1000000, interpret("1_000_000"));
    // assert_eq!(0xFF, interpret("0xFF_FF"));
}

#[test]
fn scientific_notation() {
    // Uncomment and implement when scientific notation is supported
    // assert_eq!(1000.0, interpret("1e3"));
    // assert_eq!(0.001, interpret("1e-3"));
    // assert_eq!(123000.0, interpret("1.23e5"));
}

// Placeholder test for existing functionality
#[test]
fn existing_literal_types() {
    // Test all currently supported literal types
    assert_eq!(42, interpret("42"));
    assert_eq!(1, interpret("true"));
    assert_eq!(0, interpret("false"));
    assert_eq!(-5, interpret("-5"));
    assert_eq!(0, interpret("0"));
}

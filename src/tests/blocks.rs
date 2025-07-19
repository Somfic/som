use crate::tests::interpret;

#[test]
fn simple_block() {
    assert_eq!(5, interpret("{ 5 }"));
    assert_eq!(10, interpret("{ 10 }"));
    assert_eq!(0, interpret("{ false }"));
    assert_eq!(1, interpret("{ true }"));
}

#[test]
fn block_with_statements() {
    assert_eq!(5, interpret("{ let x = 5; x }"));
    assert_eq!(3, interpret("{ let a = 1; let b = 2; a + b }"));
    assert_eq!(10, interpret("{ let x = 5; let y = x * 2; y }"));
}

#[test]
fn block_with_multiple_statements() {
    assert_eq!(15, interpret("{ let a = 5; let b = 10; a + b }"));
    assert_eq!(6, interpret("{ let x = 2; let y = 3; x * y }"));
    assert_eq!(1, interpret("{ let flag = false; let result = true; result }"));
}

#[test]
fn nested_blocks() {
    assert_eq!(10, interpret("{ { 10 } }"));
    assert_eq!(15, interpret("{ let x = 5; { let y = 10; x + y } }"));
    assert_eq!(7, interpret("{ let a = 3; { let b = 4; { a + b } } }"));
}

#[test]
fn block_scoping() {
    // Test that variables declared in inner blocks don't affect outer scope
    assert_eq!(5, interpret("{ let x = 5; { let x = 10; }; x }"));
    assert_eq!(10, interpret("{ let x = 5; { let y = 10; }; x + 5 }"));
}

#[test]
fn block_as_expression() {
    assert_eq!(8, interpret("let result = { 3 + 5 }; result"));
    assert_eq!(20, interpret("let value = { let temp = 10; temp * 2 }; value"));
}

#[test]
fn block_in_arithmetic() {
    assert_eq!(15, interpret("{ 5 + 10 }"));
    assert_eq!(8, interpret("{ 2 * 4 }"));
    assert_eq!(15, interpret("5 + { 10 }"));
    assert_eq!(25, interpret("{ 5 } * { 5 }"));
    assert_eq!(7, interpret("{ 3 + 4 }"));
}

#[test]
fn block_with_assignment() {
    assert_eq!(10, interpret("{ let x = 5; x = 10; x }"));
    assert_eq!(15, interpret("{ let a = 5; let b = 10; a = b + 5; a }"));
}

#[test]
fn block_returning_function() {
    assert_eq!(
        25,
        interpret("{ let square = fn(x ~ i32) -> i32 { x * x }; square(5) }")
    );
}

#[test]
fn empty_block_behavior() {
    // Test how empty blocks or blocks with only statements (no final expression) behave
    // This might need adjustment based on the language's actual behavior
    assert_eq!(0, interpret("{ let x = 5; }"));  // Assuming unit/empty returns 0
}

#[test]
fn block_with_conditionals() {
    assert_eq!(5, interpret("{ let x = true; x if x else 0; 5 }"));
    assert_eq!(10, interpret("{ let condition = false; 5 if condition else 10 }"));
}

use crate::tests::interpret;

#[test]
fn basic_while_loop_with_counter() {
    let program = r#"
        let i = 0;
        let sum = 0;
        while i < 5 {
            sum = sum + i;
            i = i + 1
        };
        sum
    "#;
    assert_eq!(10, interpret(program)); // 0 + 1 + 2 + 3 + 4 = 10
}

#[test]
fn while_loop_single_iteration() {
    let program = r#"
        let x = 0;
        while x < 1 {
            x = x + 1
        };
        x
    "#;
    assert_eq!(1, interpret(program));
}

#[test]
fn while_loop_zero_iterations() {
    let program = r#"
        let x = 10;
        while x < 5 {
            x = x + 1
        };
        x
    "#;
    assert_eq!(10, interpret(program)); // Loop never executes
}

#[test]
fn while_loop_counting_down() {
    let program = r#"
        let countdown = 10;
        while countdown > 0 {
            countdown = countdown - 1
        };
        countdown
    "#;
    assert_eq!(0, interpret(program));
}

#[test]
fn while_loop_with_multiplication() {
    let program = r#"
        let result = 1;
        let i = 1;
        while i < 6 {
            result = result * i;
            i = i + 1
        };
        result
    "#;
    assert_eq!(120, interpret(program)); // 5! = 120
}

#[test]
fn nested_while_loops() {
    let program = r#"
        let i = 0;
        let j = 0;
        let sum = 0;
        while i < 3 {
            j = 0;
            while j < 3 {
                sum = sum + 1;
                j = j + 1
            };
            i = i + 1
        };
        sum
    "#;
    assert_eq!(9, interpret(program)); // 3 * 3 = 9 iterations
}

#[test]
fn while_loop_with_complex_condition() {
    let program = r#"
        let x = 0;
        let y = 10;
        while x < y {
            x = x + 1;
            y = y - 1
        };
        x
    "#;
    assert_eq!(5, interpret(program)); // They meet in the middle
}

#[test]
fn while_loop_fibonacci() {
    let program = r#"
        let n = 10;
        let a = 0;
        let b = 1;
        let i = 0;
        while i < n {
            let temp = a + b;
            a = b;
            b = temp;
            i = i + 1
        };
        a
    "#;
    assert_eq!(55, interpret(program)); // 10th Fibonacci number
}

#[test]
fn while_loop_returns_unit() {
    let program = r#"
        let x = 0;
        let result = while x < 3 {
            x = x + 1
        };
        x
    "#;
    assert_eq!(3, interpret(program)); // While returns unit, but x is still mutated
}

#[test]
fn while_loop_with_boolean_condition() {
    let program = r#"
        let continue_loop = true;
        let count = 0;
        while continue_loop {
            count = count + 1;
            continue_loop = count < 5
        };
        count
    "#;
    assert_eq!(5, interpret(program));
}

#[test]
fn while_loop_modifying_multiple_variables() {
    let program = r#"
        let a = 0;
        let b = 0;
        let c = 0;
        let i = 0;
        while i < 5 {
            a = a + 1;
            b = b + 2;
            c = c + 3;
            i = i + 1
        };
        a + b + c
    "#;
    assert_eq!(30, interpret(program)); // 5 + 10 + 15 = 30
}

#[test]
fn while_loop_with_greater_than_equal() {
    let program = r#"
        let x = 10;
        while x >= 5 {
            x = x - 1
        };
        x
    "#;
    assert_eq!(4, interpret(program));
}

#[test]
fn while_loop_sum_even_numbers() {
    let program = r#"
        let sum = 0;
        let i = 0;
        while i < 10 {
            let is_even = i if i < 0 else 0;
            sum = sum + i if i >= 0 else sum;
            i = i + 2
        };
        sum
    "#;
    assert_eq!(20, interpret(program)); // 0 + 2 + 4 + 6 + 8 = 20
}

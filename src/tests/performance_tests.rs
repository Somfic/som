use crate::tests::interpret;
use crate::tests::test_helpers::expect_error;

// Performance and stress tests for the language implementation

#[test]
fn performance_large_arithmetic_chains() {
    // Test very long arithmetic expressions
    let long_addition = (0..100)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(" + ");
    let expected = (0..100).sum::<i32>() as i64;
    assert_eq!(expected, interpret(&long_addition));

    // Test long multiplication chain of small numbers
    assert_eq!(1024, interpret("2 * 2 * 2 * 2 * 2 * 2 * 2 * 2 * 2 * 2"));

    // Test long subtraction chain: 10-1=9, 9-2=7, 7-3=4, 4-4=0, 0-5=-5, -5-6=-11, -11-7=-18, -18-8=-26, -26-9=-35
    assert_eq!(-35, interpret("10 - 1 - 2 - 3 - 4 - 5 - 6 - 7 - 8 - 9"));
}

#[test]
fn performance_deeply_nested_parentheses() {
    // Test deeply nested expressions
    let depth = 20;
    let mut expr = "1".to_string();
    for _ in 0..depth {
        expr = format!("({})", expr);
    }
    assert_eq!(1, interpret(&expr));

    // Test nested arithmetic
    let nested_expr = "((((1 + 2) * 3) + 4) * 5)";
    assert_eq!(65, interpret(nested_expr)); // ((3 * 3) + 4) * 5 = (9 + 4) * 5 = 13 * 5 = 65
}

#[test]
fn performance_many_variables() {
    // Test program with many variable declarations
    let mut program = String::new();
    for i in 0..50 {
        program.push_str(&format!("let var{} = {}; ", i, i));
    }
    program.push_str("var0 + var10 + var20 + var30 + var40 + var49");
    assert_eq!(149, interpret(&program)); // 0 + 10 + 20 + 30 + 40 + 49 = 149
}

#[test]
fn performance_many_function_calls() {
    // Test many function calls in sequence
    let program = r#"
        let add1 = fn(x ~ int) -> int { x + 1 };
        let start = 0;
        let step1 = add1(start);
        let step2 = add1(step1);
        let step3 = add1(step2);
        let step4 = add1(step3);
        let step5 = add1(step4);
        let step6 = add1(step5);
        let step7 = add1(step6);
        let step8 = add1(step7);
        let step9 = add1(step8);
        let step10 = add1(step9);
        step10
    "#;
    assert_eq!(10, interpret(program));
}

#[test]
fn performance_complex_expressions() {
    // Test computationally complex expressions
    let complex_expr = r#"
        let complex = fn(a ~ int, b ~ int, c ~ int, d ~ int) -> int {
            let part1 = (a + b) * (c - d);
            let part2 = (a * c) + (b * d);
            let part3 = (a - c) * (b + d);
            part1 + part2 - part3
        };
        complex(10, 5, 8, 3)
    "#;
    // part1 = (10+5) * (8-3) = 15 * 5 = 75
    // part2 = (10*8) + (5*3) = 80 + 15 = 95
    // part3 = (10-8) * (5+3) = 2 * 8 = 16
    // result = 75 + 95 - 16 = 154
    assert_eq!(154, interpret(complex_expr));
}

#[test]
fn performance_nested_blocks() {
    // Test deeply nested block structures
    let nested_blocks = r#"
        {
            let a = 1;
            {
                let b = 2;
                {
                    let c = 3;
                    {
                        let d = 4;
                        {
                            let e = 5;
                            a + b + c + d + e
                        }
                    }
                }
            }
        }
    "#;
    assert_eq!(15, interpret(nested_blocks));
}

#[test]
fn performance_memory_intensive_operations() {
    // Test operations that might stress memory allocation
    let program = r#"
        let create_large_computation = fn(base ~ int) -> int {
            let val1 = base * 1000;
            let val2 = val1 + 500;
            let val3 = val2 * 2;
            let val4 = val3 - 1000;
            let val5 = val4 / 10;
            let val6 = val5 + 250;
            let val7 = val6 * 3;
            let val8 = val7 - 500;
            let val9 = val8 / 5;
            let val10 = val9 + 100;
            val10
        };
        
        let result1 = create_large_computation(1);
        let result2 = create_large_computation(2);
        let result3 = create_large_computation(3);
        
        result1 + result2 + result3
    "#;

    // For base = 1: 1000 -> 1500 -> 3000 -> 2000 -> 200 -> 450 -> 1350 -> 850 -> 170 -> 270
    // For base = 2: 2000 -> 2500 -> 5000 -> 4000 -> 400 -> 650 -> 1950 -> 1450 -> 290 -> 390
    // For base = 3: 3000 -> 3500 -> 7000 -> 6000 -> 600 -> 850 -> 2550 -> 2050 -> 410 -> 510
    // Total: 270 + 390 + 510 = 1170
    assert_eq!(1170, interpret(program));
}

#[test]
fn stress_test_error_conditions() {
    // Test that error handling works correctly under stress

    // Deeply nested errors
    assert!(expect_error("{ { { { undefined_var } } } }"));

    // Errors in complex expressions
    assert!(expect_error("let f = fn(x ~ int) -> int { x + y }; f(5)")); // undefined y

    // Multiple error sources
    assert!(expect_error("unknown1 + unknown2 * unknown3"));

    // Errors in function chains
    assert!(expect_error(
        "let f = fn(x ~ int) -> int { x }; let g = fn(y ~ int) -> int { f(undefined) }; g(1)"
    ));
}

#[test]
fn stress_test_boundary_arithmetic() {
    // Test arithmetic at boundaries
    assert_eq!(2147483647, interpret("2147483647")); // Max i32 if supported
    assert_eq!(-2147483648, interpret("-2147483648")); // Min i32 if supported

    // Large multiplications that stay within bounds
    assert_eq!(1000000, interpret("1000 * 1000"));
    assert_eq!(10000, interpret("100 * 100"));

    // Division with large numbers
    assert_eq!(1000, interpret("1000000 / 1000"));
    assert_eq!(100, interpret("10000 / 100"));
}

#[test]
fn stress_test_function_parameters() {
    // Test functions with many parameters
    let many_param_func = r#"
        let sum_many = fn(a ~ int, b ~ int, c ~ int, d ~ int, e ~ int, f ~ int, g ~ int, h ~ int) -> int {
            a + b + c + d + e + f + g + h
        };
        sum_many(1, 2, 3, 4, 5, 6, 7, 8)
    "#;
    assert_eq!(36, interpret(many_param_func));
}

#[test]
fn stress_test_conditional_chains() {
    // Test long chains of conditionals
    let conditional_chain = r#"
        let x = 5;
        let result = 1 if x > 4 else 2 if x > 3 else 3 if x > 2 else 4 if x > 1 else 5;
        result
    "#;
    // Since x = 5, x > 4 is true, so result should be 1
    assert_eq!(1, interpret(conditional_chain));
}

#[test]
fn stress_test_mixed_operations() {
    // Test mixing all types of operations in complex ways
    let mixed_program = r#"
        let processor = fn(input ~ int) -> int {
            let doubled = input * 2;
            let choice = doubled if doubled > 10 else input;
            let squared = choice * choice;
            let final_val = squared + choice - input;
            final_val
        };
        
        let test1 = processor(3);   // doubled=6, choice=3, squared=9, final=9+3-3=9
        let test2 = processor(8);   // doubled=16, choice=16, squared=256, final=256+16-8=264
        
        test1 + test2
    "#;
    assert_eq!(273, interpret(mixed_program)); // 9 + 264 = 273
}

#[test]
fn stress_test_scope_complexity() {
    // Test complex scoping scenarios
    let scope_test = r#"
        let global = 100;
        let func1 = fn(param ~ int) -> int {
            let local1 = param + global;
            {
                let local2 = local1 * 2;
                {
                    let local3 = local2 + param;
                    local3
                }
            }
        };
        
        func1(5)  // local1 = 5+100=105, local2 = 105*2=210, local3 = 210+5=215
    "#;
    assert_eq!(215, interpret(scope_test));
}

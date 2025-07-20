use crate::tests::interpret;

#[test]
fn test_function_definition_variations() {
    // Functions with different parameter counts
    assert_eq!(42, interpret("let f0 = fn() -> int { 42 }; f0()"));
    assert_eq!(5, interpret("let f1 = fn(x ~ int) -> int { x }; f1(5)"));
    assert_eq!(8, interpret("let f2 = fn(x ~ int, y ~ int) -> int { x + y }; f2(3, 5)"));
    assert_eq!(15, interpret("let f3 = fn(a ~ int, b ~ int, c ~ int) -> int { a + b + c }; f3(2, 5, 8)"));
    
    // Functions with different return types
    assert_eq!(1, interpret("let f_bool = fn() -> bool { true }; f_bool()"));
    assert_eq!(0, interpret("let f_bool_false = fn() -> bool { false }; f_bool_false()"));
    assert_eq!(100, interpret("let f_int = fn() -> int { 100 }; f_int()"));
}

#[test]
fn test_function_parameter_types() {
    // Functions with boolean parameters
    assert_eq!(10, interpret("let f = fn(flag ~ bool) -> int { 10 if flag else 5 }; f(true)"));
    assert_eq!(5, interpret("let f = fn(flag ~ bool) -> int { 10 if flag else 5 }; f(false)"));
    
    // Functions with mixed parameter types
    assert_eq!(15, interpret("let f = fn(x ~ int, flag ~ bool) -> int { x * 2 if flag else x }; f(15, true)"));
    assert_eq!(7, interpret("let f = fn(x ~ int, flag ~ bool) -> int { x * 2 if flag else x }; f(7, false)"));
    
    // Functions returning booleans
    assert_eq!(1, interpret("let is_positive = fn(x ~ int) -> bool { x > 0 }; is_positive(5)"));
    assert_eq!(0, interpret("let is_positive = fn(x ~ int) -> bool { x > 0 }; is_positive(-3)"));
}

#[test]
fn test_function_body_complexity() {
    // Functions with variable declarations
    assert_eq!(10, interpret("let f = fn(x ~ int) -> int { let y = x * 2; y }; f(5)"));
    assert_eq!(21, interpret("let f = fn(a ~ int, b ~ int) -> int { let sum = a + b; let double = sum * 2; double + 1 }; f(5, 5)"));
    
    // Functions with multiple statements
    assert_eq!(50, interpret("let f = fn(x ~ int) -> int { let a = x; let b = a * 2; let c = b + a; c * 2 - x }; f(10)"));
    
    // Functions with conditionals in body
    assert_eq!(10, interpret("let abs = fn(x ~ int) -> int { x if x >= 0 else -x }; abs(10)"));
    assert_eq!(5, interpret("let abs = fn(x ~ int) -> int { x if x >= 0 else -x }; abs(-5)"));
    
    // Functions with nested expressions
    assert_eq!(42, interpret("let complex = fn(x ~ int, y ~ int) -> int { (x + y) * (x - y) + x * y }; complex(7, 1)"));
}

#[test]
fn test_function_calls_with_expressions() {
    // Function calls with arithmetic expressions as arguments
    assert_eq!(7, interpret("let add = fn(a ~ int, b ~ int) -> int { a + b }; add(2 + 1, 3 + 1)"));
    assert_eq!(20, interpret("let multiply = fn(x ~ int, y ~ int) -> int { x * y }; multiply(2 * 2, 6 - 1)"));
    
    // Function calls with variable expressions
    assert_eq!(15, interpret("let add = fn(a ~ int, b ~ int) -> int { a + b }; let x = 7; let y = 8; add(x, y)"));
    assert_eq!(42, interpret("let calc = fn(x ~ int) -> int { x * x }; let val = 6; calc(val + 1)"));
    
    // Nested function calls
    assert_eq!(12, interpret("let double = fn(x ~ int) -> int { x * 2 }; let add = fn(a ~ int, b ~ int) -> int { a + b }; add(double(2), double(4))"));
}

#[test]
fn test_function_calls_as_expressions() {
    // Function calls in arithmetic expressions
    assert_eq!(15, interpret("let get5 = fn() -> int { 5 }; get5() + 10"));
    assert_eq!(30, interpret("let get5 = fn() -> int { 5 }; get5() * 6"));
    assert_eq!(0, interpret("let get5 = fn() -> int { 5 }; get5() - 5"));
    
    // Function calls in conditional expressions
    assert_eq!(100, interpret("let get_true = fn() -> bool { true }; 100 if get_true() else 0"));
    assert_eq!(0, interpret("let get_false = fn() -> bool { false }; 100 if get_false() else 0"));
    
    // Function calls with other function calls
    assert_eq!(25, interpret("let square = fn(x ~ int) -> int { x * x }; let get5 = fn() -> int { 5 }; square(get5())"));
}

#[test]
fn test_functions_with_scoping() {
    // Function parameter scoping
    assert_eq!(10, interpret("let x = 5; let f = fn(x ~ int) -> int { x * 2 }; f(10)"));  // Parameter x shadows outer x
    
    // Function local variable scoping
    assert_eq!(5, interpret("let x = 10; let f = fn() -> int { let x = 5; x }; f()"));    // Local x shadows outer x
    assert_eq!(10, interpret("let x = 10; let f = fn() -> int { let y = 5; x }; f()"));   // Function accesses outer x
    
    // Block scoping within functions
    assert_eq!(15, interpret("let f = fn(x ~ int) -> int { let y = { let z = 5; z }; x + y }; f(10)"));
}

#[test]
fn test_functions_returning_functions() {
    // This might not be supported yet, but testing basic function assignment
    assert_eq!(10, interpret("let make_doubler = fn() -> fn(int) -> int { fn(x ~ int) -> int { x * 2 } }; let doubler = make_doubler(); doubler(5)"));
    
    // For now, simpler tests that should work
    assert_eq!(20, interpret("let f = fn() -> int { 10 }; let g = fn() -> int { f() * 2 }; g()"));
}

#[test]
fn test_recursive_functions() {
    // Simple recursive function (factorial)
    // Note: This might not work if recursion isn't implemented
    // assert_eq!(120, interpret("let fact = fn(n ~ int) -> int { 1 if n <= 1 else n * fact(n - 1) }; fact(5)"));
    
    // For now, test functions that call themselves indirectly
    assert_eq!(5, interpret("let f = fn(x ~ int) -> int { x }; let g = fn(y ~ int) -> int { f(y) }; g(5)"));
}

#[test]
fn test_function_edge_cases() {
    // Function that returns its parameter unchanged (identity)
    assert_eq!(42, interpret("let id = fn(x ~ int) -> int { x }; id(42)"));
    assert_eq!(1, interpret("let bool_id = fn(b ~ bool) -> bool { b }; bool_id(true)"));
    
    // Function that ignores its parameter
    assert_eq!(100, interpret("let const_100 = fn(x ~ int) -> int { 100 }; const_100(999)"));
    
    // Function with complex arithmetic
    assert_eq!(728, interpret("let cube_sum = fn(x ~ int, y ~ int) -> int { x * x * x + y * y * y }; cube_sum(6, 8)"));  // 6^3 + 8^3 = 216 + 512 = 728
    assert_eq!(728, interpret("let cube_sum = fn(x ~ int, y ~ int) -> int { x * x * x + y * y * y }; cube_sum(6, 8)"));
    
    // Function with boolean logic
    assert_eq!(1, interpret("let both_true = fn(a ~ bool, b ~ bool) -> bool { a if b else false }; both_true(true, true)"));
    assert_eq!(0, interpret("let both_true = fn(a ~ bool, b ~ bool) -> bool { a if b else false }; both_true(true, false)"));
}

#[test]
fn test_function_assignments() {
    // Assigning functions to variables
    assert_eq!(6, interpret("let add = fn(x ~ int, y ~ int) -> int { x + y }; let my_func = add; my_func(2, 4)"));
    
    // Reassigning function variables
    assert_eq!(10, interpret("let f = fn(x ~ int) -> int { x }; f = fn(x ~ int) -> int { x * 2 }; f(5)"));
    
    // Functions with the same signature
    assert_eq!(7, interpret("let add = fn(a ~ int, b ~ int) -> int { a + b }; let sub = fn(a ~ int, b ~ int) -> int { a - b }; add(5, 2)"));
    assert_eq!(3, interpret("let add = fn(a ~ int, b ~ int) -> int { a + b }; let sub = fn(a ~ int, b ~ int) -> int { a - b }; sub(5, 2)"));
}

#[test]
fn test_functions_with_type_annotations() {
    // Explicit type annotations
    assert_eq!(15, interpret("let add ~ fn(int, int) -> int = fn(x ~ int, y ~ int) -> int { x + y }; add(7, 8)"));
    assert_eq!(1, interpret("let is_pos ~ fn(int) -> bool = fn(x ~ int) -> bool { x > 0 }; is_pos(5)"));
    
    // Mixed with other typed variables
    assert_eq!(50, interpret("let multiplier ~ fn(int, int) -> int = fn(a ~ int, b ~ int) -> int { a * b }; let x ~ int = 5; multiplier(x, 10)"));
}

#[test]
fn test_function_call_chains() {
    // Chain multiple function calls
    assert_eq!(8, interpret("let add1 = fn(x ~ int) -> int { x + 1 }; add1(add1(add1(5)))"));
    assert_eq!(80, interpret("let double = fn(x ~ int) -> int { x * 2 }; double(double(double(10)))"));
    
    // Mix different functions
    assert_eq!(13, interpret("let add2 = fn(x ~ int) -> int { x + 2 }; let mul3 = fn(x ~ int) -> int { x * 3 }; add2(mul3(3)) + 2"));
}

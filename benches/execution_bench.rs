use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use som::{Compiler, Lexer, Parser, Runner, TypeChecker};

/// Helper function to compile and execute som source code
fn execute_source(source: &str) -> Result<i64, String> {
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);
    let parsed = parser.parse().map_err(|e| format!("{:?}", e))?;

    let mut type_checker = TypeChecker::new();
    let type_checked = type_checker.check(&parsed).map_err(|e| format!("{:?}", e))?;

    let mut compiler = Compiler::new();
    let (compiled, return_type) = compiler.compile(&type_checked).map_err(|e| format!("{:?}", e))?;

    let runner = Runner::new();
    runner.run(compiled, &return_type).map_err(|e| format!("{:?}", e))
}

/// Benchmark basic arithmetic operations
fn bench_arithmetic_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/arithmetic");

    let programs = vec![
        ("addition", "1 + 2 + 3 + 4 + 5"),
        ("multiplication", "2 * 3 * 4 * 5"),
        ("mixed", "(10 + 5) * (20 - 8) / 3"),
        ("complex", "((100 + 50) * 2 - 75) / 5 + 10"),
    ];

    for (name, source) in programs {
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, &s| {
            b.iter(|| execute_source(black_box(s)))
        });
    }

    group.finish();
}

/// Benchmark function call overhead
fn bench_function_calls(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/function_calls");

    let simple_call = r#"
        let add = fn(a ~ int, b ~ int) -> int { a + b };
        add(10, 20)
    "#;

    let nested_calls = r#"
        let add = fn(a ~ int, b ~ int) -> int { a + b };
        let mul = fn(a ~ int, b ~ int) -> int { a * b };
        mul(add(5, 10), add(3, 7))
    "#;

    let many_params = r#"
        let sum = fn(a ~ int, b ~ int, c ~ int, d ~ int, e ~ int) -> int {
            a + b + c + d + e
        };
        sum(1, 2, 3, 4, 5)
    "#;

    group.bench_function("simple_call", |b| {
        b.iter(|| execute_source(black_box(simple_call)))
    });

    group.bench_function("nested_calls", |b| {
        b.iter(|| execute_source(black_box(nested_calls)))
    });

    group.bench_function("many_parameters", |b| {
        b.iter(|| execute_source(black_box(many_params)))
    });

    group.finish();
}

/// Benchmark recursion performance
fn bench_recursion(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/recursion");

    // Factorial at different depths
    for n in [5, 10, 15, 20].iter() {
        let program = format!(
            r#"
            let factorial = fn(n ~ int) -> int {{
                1 if n < 2 else n * factorial(n - 1)
            }};
            factorial({})
        "#,
            n
        );

        group.bench_with_input(BenchmarkId::new("factorial", n), &program, |b, prog| {
            b.iter(|| execute_source(black_box(prog)))
        });
    }

    // Fibonacci at different depths (careful - fib is exponential!)
    for n in [5, 10, 15, 20].iter() {
        let program = format!(
            r#"
            let fib = fn(n ~ int) -> int {{
                n if n < 2 else fib(n - 1) + fib(n - 2)
            }};
            fib({})
        "#,
            n
        );

        group.bench_with_input(BenchmarkId::new("fibonacci", n), &program, |b, prog| {
            b.iter(|| execute_source(black_box(prog)))
        });
    }

    group.finish();
}

/// Benchmark simple recursion at deeper depths
fn bench_deep_recursion(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/deep_recursion");
    group.sample_size(30);

    // Simple countdown recursion
    for n in [50, 100, 200].iter() {
        let program = format!(
            r#"
            let countdown = fn(n ~ int) -> int {{
                n if n < 1 else countdown(n - 1)
            }};
            countdown({})
        "#,
            n
        );

        group.bench_with_input(BenchmarkId::new("countdown", n), &program, |b, prog| {
            b.iter(|| execute_source(black_box(prog)))
        });
    }

    // Sum recursion
    for n in [30, 50, 100].iter() {
        let program = format!(
            r#"
            let sum = fn(n ~ int) -> int {{
                0 if n < 1 else n + sum(n - 1)
            }};
            sum({})
        "#,
            n
        );

        group.bench_with_input(BenchmarkId::new("sum", n), &program, |b, prog| {
            b.iter(|| execute_source(black_box(prog)))
        });
    }

    group.finish();
}

/// Benchmark while loop performance
fn bench_while_loops(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/while_loops");

    for iterations in [10, 50, 100, 500].iter() {
        let program = format!(
            r#"
            let sum = fn(n ~ int) -> int {{
                let acc = 0;
                let i = 0;
                while i < n {{
                    let acc = acc + i;
                    let i = i + 1;
                }};
                acc
            }};
            sum({})
        "#,
            iterations
        );

        group.bench_with_input(BenchmarkId::new("sum_loop", iterations), &program, |b, prog| {
            b.iter(|| execute_source(black_box(prog)))
        });
    }

    group.finish();
}

/// Benchmark conditional expressions
fn bench_conditionals(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/conditionals");

    let simple_if = r#"
        let x = 10;
        42 if x > 5 else 0
    "#;

    let nested_if = r#"
        let x = 15;
        (100 if x > 20 else 50) if x > 10 else (25 if x > 5 else 0)
    "#;

    let chained_if = r#"
        let x = 75;
        10 if x > 90 else 9 if x > 80 else 8 if x > 70 else 7 if x > 60 else 5
    "#;

    group.bench_function("simple", |b| {
        b.iter(|| execute_source(black_box(simple_if)))
    });

    group.bench_function("nested", |b| {
        b.iter(|| execute_source(black_box(nested_if)))
    });

    group.bench_function("chained", |b| {
        b.iter(|| execute_source(black_box(chained_if)))
    });

    group.finish();
}

/// Benchmark complex real-world scenarios
fn bench_complex_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("execution/complex");
    group.sample_size(30);

    // GCD using Euclidean algorithm
    let gcd = r#"
        let gcd = fn(a ~ int, b ~ int) -> int {
            a if b == 0 else gcd(b, a - (a / b) * b)
        };
        gcd(48, 18)
    "#;

    // Ackermann function (very computationally intensive)
    let ackermann = r#"
        let ackermann = fn(m ~ int, n ~ int) -> int {
            (n + 1) if m == 0 else (
                ackermann(m - 1, 1) if n == 0 else
                ackermann(m - 1, ackermann(m, n - 1))
            )
        };
        ackermann(3, 4)
    "#;

    // Collatz sequence
    let collatz = r#"
        let collatz = fn(n ~ int) -> int {
            1 if n == 1 else (
                collatz(n / 2) if n - (n / 2) * 2 == 0 else
                collatz(n * 3 + 1)
            )
        };
        collatz(27)
    "#;

    group.bench_function("gcd", |b| {
        b.iter(|| execute_source(black_box(gcd)))
    });

    group.bench_function("ackermann", |b| {
        b.iter(|| execute_source(black_box(ackermann)))
    });

    group.bench_function("collatz", |b| {
        b.iter(|| execute_source(black_box(collatz)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_arithmetic_operations,
    bench_function_calls,
    bench_recursion,
    bench_deep_recursion,
    bench_while_loops,
    bench_conditionals,
    bench_complex_programs
);
criterion_main!(benches);

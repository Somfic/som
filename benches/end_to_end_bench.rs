use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use som::{Compiler, Lexer, Parser, Runner, TypeChecker};

/// Full pipeline: lex -> parse -> typecheck -> compile -> execute
fn full_pipeline(source: &str) -> Result<i64, String> {
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

/// Benchmark realistic programs end-to-end
fn bench_realistic_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end/realistic");

    // Simple calculator
    let calculator = r#"
        let add = fn(a ~ int, b ~ int) -> int { a + b };
        let sub = fn(a ~ int, b ~ int) -> int { a - b };
        let mul = fn(a ~ int, b ~ int) -> int { a * b };
        let div = fn(a ~ int, b ~ int) -> int { a / b };

        let result = add(mul(10, 5), sub(20, div(8, 2)));
        result
    "#;

    // Fibonacci computation
    let fibonacci = r#"
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };
        fib(15)
    "#;

    // GCD algorithm
    let gcd = r#"
        let gcd = fn(a ~ int, b ~ int) -> int {
            a if b == 0 else gcd(b, a - (a / b) * b)
        };
        gcd(48, 18)
    "#;

    // Ackermann function
    let ackermann = r#"
        let ackermann = fn(m ~ int, n ~ int) -> int {
            (n + 1) if m == 0 else (
                ackermann(m - 1, 1) if n == 0 else
                ackermann(m - 1, ackermann(m, n - 1))
            )
        };
        ackermann(3, 4)
    "#;

    group.bench_function("calculator", |b| {
        b.iter(|| full_pipeline(black_box(calculator)))
    });

    group.bench_function("fibonacci", |b| {
        b.iter(|| full_pipeline(black_box(fibonacci)))
    });

    group.bench_function("gcd", |b| {
        b.iter(|| full_pipeline(black_box(gcd)))
    });

    group.bench_function("ackermann", |b| {
        b.iter(|| full_pipeline(black_box(ackermann)))
    });

    group.finish();
}

/// Benchmark programs with different recursion depths
fn bench_recursion_depths(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end/recursion_depth");

    // Non-tail-recursive (limited depth due to stack)
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

        group.bench_with_input(
            BenchmarkId::new("non_tail_recursive", n),
            &program,
            |b, prog| b.iter(|| full_pipeline(black_box(prog))),
        );
    }

    // Simple recursion (sum)
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

        group.bench_with_input(BenchmarkId::new("simple_recursive", n), &program, |b, prog| {
            b.iter(|| full_pipeline(black_box(prog)))
        });
    }

    group.finish();
}

/// Benchmark programs with different complexity levels
fn bench_program_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end/complexity");

    // Small program
    let small = r#"
        let x = 42;
        let y = x + 8;
        y * 2
    "#;

    // Medium program with functions
    let medium = r#"
        let square = fn(x ~ int) -> int { x * x };
        let sum_squares = fn(a ~ int, b ~ int) -> int {
            square(a) + square(b)
        };

        let result = sum_squares(10, 20);
        result
    "#;

    // Large program with multiple features
    let large = r#"
        let is_even = fn(n ~ int) -> int {
            0 if n - (n / 2) * 2 == 0 else 1
        };

        let factorial = fn(n ~ int) -> int {
            1 if n < 2 else n * factorial(n - 1)
        };

        let process = fn(n ~ int) -> int {
            let doubled = n * 2;
            let adjusted = doubled if is_even(n) == 0 else doubled + 1;
            let fact_result = factorial(adjusted if adjusted < 10 else 10);
            fact_result
        };

        process(7)
    "#;

    group.bench_function("small", |b| {
        b.iter(|| full_pipeline(black_box(small)))
    });

    group.bench_function("medium", |b| {
        b.iter(|| full_pipeline(black_box(medium)))
    });

    group.bench_function("large", |b| {
        b.iter(|| full_pipeline(black_box(large)))
    });

    group.finish();
}

/// Benchmark programs with loops
fn bench_loop_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end/loops");

    for iterations in [10, 50, 100, 200].iter() {
        let program = format!(
            r#"
            let sum = fn(n ~ int) -> int {{
                let total = 0;
                let i = 0;
                while i < n {{
                    let total = total + i;
                    let i = i + 1;
                }};
                total
            }};
            sum({})
        "#,
            iterations
        );

        group.bench_with_input(BenchmarkId::new("sum_loop", iterations), &program, |b, prog| {
            b.iter(|| full_pipeline(black_box(prog)))
        });
    }

    // Nested loops
    let nested_loop = r#"
        let sum = fn(n ~ int) -> int {
            let total = 0;
            let i = 0;
            while i < n {
                let j = 0;
                while j < n {
                    let total = total + 1;
                    let j = j + 1;
                };
                let i = i + 1;
            };
            total
        };
        sum(20)
    "#;

    group.bench_function("nested_loop", |b| {
        b.iter(|| full_pipeline(black_box(nested_loop)))
    });

    group.finish();
}

/// Benchmark classic algorithms
fn bench_classic_algorithms(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end/algorithms");
    group.sample_size(30);

    // GCD (Euclidean algorithm)
    let gcd = r#"
        let gcd = fn(a ~ int, b ~ int) -> int {
            a if b == 0 else gcd(b, a - (a / b) * b)
        };
        gcd(1071, 462)
    "#;

    // Power function
    let power = r#"
        let power = fn(base ~ int, exp ~ int) -> int {
            1 if exp < 1 else base * power(base, exp - 1)
        };
        power(2, 15)
    "#;

    // Collatz conjecture
    let collatz = r#"
        let collatz = fn(n ~ int) -> int {
            1 if n == 1 else (
                collatz(n / 2) if n - (n / 2) * 2 == 0 else
                collatz(n * 3 + 1)
            )
        };
        collatz(27)
    "#;

    // Triangle numbers
    let triangle = r#"
        let triangle = fn(n ~ int) -> int {
            0 if n < 1 else n + triangle(n - 1)
        };
        triangle(100)
    "#;

    group.bench_function("gcd", |b| {
        b.iter(|| full_pipeline(black_box(gcd)))
    });

    group.bench_function("power", |b| {
        b.iter(|| full_pipeline(black_box(power)))
    });

    group.bench_function("collatz", |b| {
        b.iter(|| full_pipeline(black_box(collatz)))
    });

    group.bench_function("triangle", |b| {
        b.iter(|| full_pipeline(black_box(triangle)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_realistic_programs,
    bench_recursion_depths,
    bench_program_complexity,
    bench_loop_programs,
    bench_classic_algorithms
);
criterion_main!(benches);

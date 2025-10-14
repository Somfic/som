use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use som::{Compiler, Lexer, Parser, TypeChecker};

/// Helper function to compile som source code through parse + typecheck + compile
fn compile_source(source: &str) -> Result<(), String> {
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);
    let parsed = parser.parse().map_err(|e| format!("{:?}", e))?;

    let mut type_checker = TypeChecker::new();
    let type_checked = type_checker.check(&parsed).map_err(|e| format!("{:?}", e))?;

    let mut compiler = Compiler::new();
    compiler.compile(&type_checked).map_err(|e| format!("{:?}", e))?;

    Ok(())
}

/// Benchmark small programs (simple expressions)
fn bench_small_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation/small");

    let programs = vec![
        ("arithmetic", "1 + 2 * 3"),
        ("variable", "let x = 42; x"),
        ("simple_function", "let f = fn(x ~ int) -> int { x + 1 }; f(5)"),
        ("conditional", "let x = 10; 42 if x > 5 else 0"),
    ];

    for (name, source) in programs {
        group.bench_with_input(BenchmarkId::from_parameter(name), &source, |b, &s| {
            b.iter(|| compile_source(black_box(s)))
        });
    }

    group.finish();
}

/// Benchmark medium-sized programs
fn bench_medium_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation/medium");

    let factorial = r#"
        let factorial = fn(n ~ int) -> int {
            1 if n < 2 else n * factorial(n - 1)
        };
        factorial(10)
    "#;

    let fibonacci = r#"
        let fib = fn(n ~ int) -> int {
            n if n < 2 else fib(n - 1) + fib(n - 2)
        };
        fib(10)
    "#;

    let nested_blocks = r#"
        {
            let a = 1;
            {
                let b = 2;
                {
                    let c = 3;
                    {
                        let d = 4;
                        a + b + c + d
                    }
                }
            }
        }
    "#;

    let multiple_functions = r#"
        let add = fn(a ~ int, b ~ int) -> int { a + b };
        let sub = fn(a ~ int, b ~ int) -> int { a - b };
        let mul = fn(a ~ int, b ~ int) -> int { a * b };
        let div = fn(a ~ int, b ~ int) -> int { a / b };
        add(10, sub(20, mul(3, div(8, 2))))
    "#;

    group.bench_function("factorial", |b| {
        b.iter(|| compile_source(black_box(factorial)))
    });

    group.bench_function("fibonacci", |b| {
        b.iter(|| compile_source(black_box(fibonacci)))
    });

    group.bench_function("nested_blocks", |b| {
        b.iter(|| compile_source(black_box(nested_blocks)))
    });

    group.bench_function("multiple_functions", |b| {
        b.iter(|| compile_source(black_box(multiple_functions)))
    });

    group.finish();
}

/// Benchmark large/complex programs
fn bench_large_programs(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation/large");
    group.sample_size(50); // Reduce sample size for slower benchmarks

    // Complex multi-parameter function
    let complex_multiarg = r#"
        let compute = fn(a ~ int, b ~ int, c ~ int, d ~ int) -> int {
            let part1 = (a + b) * (c - d);
            let part2 = (a * c) + (b * d);
            let part3 = (a - c) * (b + d);
            part1 + part2 - part3
        };
        compute(10, 5, 8, 3)
    "#;

    // Ackermann function (complex recursion)
    let ackermann = r#"
        let ackermann = fn(m ~ int, n ~ int) -> int {
            (n + 1) if m == 0 else (
                ackermann(m - 1, 1) if n == 0 else
                ackermann(m - 1, ackermann(m, n - 1))
            )
        };
        ackermann(3, 3)
    "#;

    // Many variables
    let many_vars: String = {
        let mut program = String::new();
        for i in 0..100 {
            program.push_str(&format!("let var{} = {}; ", i, i));
        }
        program.push_str("var0 + var50 + var99");
        program
    };

    // Long arithmetic chain
    let long_arithmetic: String = {
        (0..200)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" + ")
    };

    // Complex nested conditionals
    let nested_conditionals = r#"
        let classify = fn(x ~ int) -> int {
            (
                10 if x > 90 else 9 if x > 80 else 8 if x > 70 else
                7 if x > 60 else 6 if x > 50 else 5 if x > 40 else
                4 if x > 30 else 3 if x > 20 else 2 if x > 10 else 1
            )
        };
        classify(75)
    "#;

    group.bench_function("complex_multiarg", |b| {
        b.iter(|| compile_source(black_box(complex_multiarg)))
    });

    group.bench_function("ackermann", |b| {
        b.iter(|| compile_source(black_box(ackermann)))
    });

    group.bench_function("many_variables", |b| {
        b.iter(|| compile_source(black_box(&many_vars)))
    });

    group.bench_function("long_arithmetic", |b| {
        b.iter(|| compile_source(black_box(&long_arithmetic)))
    });

    group.bench_function("nested_conditionals", |b| {
        b.iter(|| compile_source(black_box(nested_conditionals)))
    });

    group.finish();
}

/// Benchmark while loops
fn bench_while_loops(c: &mut Criterion) {
    let mut group = c.benchmark_group("compilation/loops");

    let simple_while = r#"
        let x = 0;
        while x < 10 {
            let x = x + 1;
        }
    "#;

    let nested_while = r#"
        let i = 0;
        while i < 10 {
            let j = 0;
            while j < 10 {
                let j = j + 1;
            };
            let i = i + 1;
        }
    "#;

    group.bench_function("simple_while", |b| {
        b.iter(|| compile_source(black_box(simple_while)))
    });

    group.bench_function("nested_while", |b| {
        b.iter(|| compile_source(black_box(nested_while)))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_small_programs,
    bench_medium_programs,
    bench_large_programs,
    bench_while_loops
);
criterion_main!(benches);

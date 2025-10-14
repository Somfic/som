# som

> An idiot admires complexity, a genius admires simplicity.

```rust
let fib = fn(n ~ int) {
    n if n < 2 else fib(n - 1) + fib(n - 2)
}

fib(10)
```

```rust
type Option<T> = Some(T) | None

type Rgb = { r ~ int, g ~ int, b ~ int }

type Color = Red 
           | Green 
           | Blue 
           | Hex(string) 
           | Rgb(Rgb)

let print_color = fn(color ~ Color) print(color)
```

## Performance Benchmarking

Som includes comprehensive benchmarks to track compilation and execution performance:

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark suite
cargo bench --bench compilation_bench
cargo bench --bench execution_bench

# Or use the helper script
./scripts/bench.sh all
./scripts/bench.sh compilation

# Save baseline and compare
./scripts/bench.sh save main
# ... make changes ...
./scripts/bench.sh compare main
```

See [BENCHMARKING.md](BENCHMARKING.md) for detailed documentation.

## TODO

- `<=` (less than or equal) operator
- `!=` (not equal) operator
- Boolean equality comparison (type system limitation)
- Recursion (multiple TODOs in test files)
- Division by zero handling (platform-specific signal handlers needed)
- Extended literals (hex, octal, binary)
- Loops (while, for)
- Type coercion between integer types
- String operations (lexer tests exist, but no integration tests)
- Pattern matching
- Enums and algebraic data types
- Structs
- Modules and imports
- Error recovery

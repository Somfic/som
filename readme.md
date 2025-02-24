# som

> An idiot admires complexity, a genius admires simplicity.

```rust
fn split(text ~ string, split ~ character) -> string, string {
    string::split(text, character)
}
```

```rust
fn fib(n ~ int) -> int {
    n if n < 2 else fib(n - 1) + fib(n - 2)
}
```

### Building project

1. Install CMake

```
cargo install llvmenv
```

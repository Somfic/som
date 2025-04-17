# som

> An idiot admires complexity, a genius admires simplicity.

```rust
let fib = fn(n ~ int) -> int {
    n if n < 2 else fib(n - 1) + fib(n - 2)
}

fib(10)
```

```rust
let Option<T> = enum Some(T) | None

let Color = enum Red | Green | Blue 
            | Hex(string) 
            | Rgb(Rgb)

let Rgb = type { r ~ int, g ~ int, b ~ int }

fn print_color(color ~ Color)
    print(color)
```

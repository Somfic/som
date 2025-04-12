# som

> An idiot admires complexity, a genius admires simplicity.

```rust
fn main() {
    fib(10)
}

fn fib(n ~ int) ~ int {
    n if n < 2 else fib(n - 1) + fib(n - 2)
}
```

```rust
type Option<T> = Some(T)
               | None

type Color = Red 
           | Green 
           | Blue 
           | Hex(string) 
           | Rgb(Rgb)

type Rgb = { r ~ int, g ~ int, b ~ int }

fn print_color(color ~ Color)
    print(color)
```

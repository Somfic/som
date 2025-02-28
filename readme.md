# som

> An idiot admires complexity, a genius admires simplicity.

```rust
fn split(text ~ string, split ~ character) -> string, string {
    string::split(text, character)
}
```

```rust
fn fib(n ~ int) -> int
    1 if n <= 1 else fib(n - 1) + fib(n - 2)
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

# som

> An idiot admires complexity, a genius admires simplicity.

```rust
let fib = fn(n ~ int){
    n if n < 2 else fib(n - 1) + fib(n - 2)
}
 -> int 
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

fn print_color(color ~ Color)
    print(color)
```

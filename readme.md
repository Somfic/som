# som

> An idiot admires complexity, a genius admires simplicity.

```rust
let fib = fn(n: int) {
    n if n < 2 else fib(n - 1) + fib(n - 2)
};

fib(10);
```

```rust
type Option<T> = Some(T) | None

type Rgb = { r: int, g: int, b: int }

type Color = Red 
           | Green 
           | Blue 
           | Hex(string) 
           | Rgb(Rgb)

let print_color = fn(color: Color) print(color)
```

## Fragments

Fragments are reusable fields for structs and enum members.

```rust
frag VesselFragment {
    name: string,
    capacity: int,
}

enum Vessel {
    Ship(...VesselFragment, crew: int),
    Boat(...VesselFragment),
    Train(...VesselFragment, cars: int),
}


let ship = Vessel::Ship {
    name: "Black Pearl",
    capacity: 500,
    crew: 100,
};

let name = ship.name; // Common fields are accessible directly

let crew = when ship {
    Vessel::Ship { crew } => crew, // Specific field accessed via pattern matching
    _ => 0,
};
```

## Generics

```rust
type Pair<T, U> = { first: T, second: U }
let int_pair: Pair<int, int> = { first: 1, second: 2 }

# dyn

*Status (2026-11-24): `dyn Trait` as a type, made from any value implementing the trait
where one is expected, its methods called through the functions it holds, in code compiled
and run while compiling. Tests: `tests/corpus/dyn_traits`.*

A `dyn Shape` is a value of any type implementing `Shape`: which one is known only as the
program runs. A list of them holds circles and squares; a function can give either.

```som
trait Shape {
    pub fn area(&self) -> f64;
}

fn pick(round: bool) -> dyn Shape {
    if round {
        return Circle { r: 1.0 };
    }
    Square { side: 2.0 }
}

let mut shapes: List<dyn Shape> = List::empty();
shapes.push(Circle { r: 1.0 });
shapes.push(Square { side: 2.0 });
for s in &shapes {
    println("{s.area()}");
}
```

- **It owns what it holds**, on the heap: moved, not copied, and dropping it drops that.
  It's a value of its own, so it goes anywhere a type does: `List<dyn Shape>`, a field,
  `&dyn Shape`, `&mut dyn Shape`, `#dyn Shape`.
- **It's made where one is expected**: a `Circle` passed for a `dyn Shape`. Numbers and
  text nothing's decided the type of are their usual types: `let d: dyn Display = 42` holds
  an `i32`.
- **It implements its trait**: a generic function requiring `Shape` takes one, and
  `dyn Display` is written into text as what it holds is.
- **Which traits**: each method a `dyn` holds, those implementations write, has to take
  `&self` or `&mut self`, and nothing else of type `Self`, nor have type parameters of its
  own; the trait can't have associated types or build on traits with methods. Otherwise
  it's an error at the `dyn` (`NotDyn`), naming the method. A method with a body in the
  trait is called with `Self` the `dyn`, through the others.

Compared with a generic function, `fn f<T: Shape>`: that's compiled for each type it's used
with, so one call takes one type; a `dyn` takes any, chosen as the program runs, at the cost
of a call through an address and the value on the heap.

## How it's built

- **The checker**: `Ty::Dyn(trait, args)`; a value made one where one is expected is
  recorded (`Typed::made_dyn`). `crates/compiler/4-typeck/src/check/dyns.rs`.
- **In MIR it's a struct** (`crates/compiler/6-lower/src/dyns.rs`), as a closure is:
  `value: *`, where the value is; `type: u64`, a number for its type; `drop: fn(*)`; then a
  `fn(*, …) -> …` for each method. Each type made a `dyn` gets a function per method,
  calling its implementation with the `*` as its `&self`.
- **A call of a method with `Self` a `dyn`** is a function reading the one it holds and
  calling through it. Everything after lowering (borrow checking, running while compiling,
  C) sees an ordinary struct of pointers.
- **`Error` holds a `dyn Display`**: a `fail NotFound { .. }` arm compares the number with
  `NotFound`'s, then reads the value through the `*`.

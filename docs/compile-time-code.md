# Compile-time code

*Design spec. Status: steps 0–12 done, step 13's `@for` around items, and step 14's
`@for (i, v) in Kind` among `match` arms, in `crates/compiler/8-comptime`: `@const` with
calls, loops, recursion, heap memory and text while compiling; structs, enums, arrays,
optionals and text as values; constants in impl blocks and function bodies; `@const` as an
expression; `@assert` and `@fail`; `@const` once per generic instance; and `@for` and `@if`,
including over a generic function's or impl block's `N`. The rest is proposed.*

Som code can already be worked out while compiling, but only a little of it: a `const` can be
literals, operators, casts and other constants, and nothing else. This spec lets **any som
code run while compiling**, written in the same som as the rest of the program. The same
`fn` works in both places: called at run time it runs then, called from an `@const` it runs
while compiling, and only its result ends up in the program.

The idea comes from Zig (`comptime`, `inline for`) and Jai (`#run`, `#insert`). Som's
version is built on what it already has: `const`, value generics like `<N: usize>`, and `@`
for things the compiler does (`@where`, `@unreachable`).

---

## 1. The syntax

There are four things, and each has a single meaning:

| Written | Means | Runs |
|---|---|---|
| `@const X = expr;` | A named constant. `expr` can now call functions | While compiling |
| `@const expr` / `@const { … }` | This value, worked out where it's written | While compiling |
| `@f(args)` / `@a::f(args)` | A call of an ordinary `fn`, the same as `@const f(args)`. On its own at the top level, a check: `@assert(…);`. A built-in's name (`@fail`, `@embed`, …) always means the built-in | While compiling |
| `@for x in xs { … }` | One copy of the body for each `x` | The loop while compiling, the copies at run time |
| `@if cond { … } else { … }` | Only the branch `cond` picks exists | The choice while compiling, the branch at run time |

The rule: **`@` marks the parts of the program the compiler settles statically.** That
includes named constants, so the plain `const` keyword goes away: every constant is
`@const X = …`, from `@const CELLS = COLS * ROWS;` to `@const SHAPES = all_rotations();`.
There's one spelling, and reading any line tells you what's settled while compiling.

`@const` evaluates. `@for` and `@if` don't run their bodies while compiling: they decide which
code exists. In Zig these are `comptime { }` and `inline for`.

### Grammar

```
item  := …
       | ("pub" | "mod")? "@const" name (":" type)? "=" expr ";"
       | "@" path "(" args ")" ";"   // a check run while compiling: `@assert(…);`

expr  := …
       | "@const" unary_expr          // binds like `-` and `!`
       | "@const" block
       | "@" path "(" args ")"        // a call while compiling; `path` isn't a built-in

stmt  := …
       | "@for" pattern "in" expr block
       | "@if" expr block ("else" (block | stmt_if_static))?
```

A declaration and an expression both start with `@const`. In a function body the parser
tells them apart by what follows: a name and then `:` or `=` is a declaration
(`@const LIMIT = 10;`), and anything else is an expression (`@const f(x)`).

`@const` binds like a prefix operator, so `@const f(x) + 1` is `(@const f(x)) + 1`. To cover
more, use a block or parentheses. This keeps it from silently swallowing the rest of a line.

### What counts as statically known

A *static* expression is one the compiler can work out without running the program:

- literals, and arrays and structs of statically known values
- constants, and `usize::MAX` and the other limits
- a generic value parameter (`N` in `impl<N: usize, T>`), in monomorphized code
- calls whose arguments are all statically known
- `@const` anything
- the built-ins in [section 5](#5-built-ins)

`@for`'s iterable, `@if`'s condition, and everything an `@const` uses must be
static. Naming a local or a parameter there is a `ConstNotKnown` error that points at the
name: "`n` is only known when the program runs".

### Replacing `const`

Every `const` in the std library, the examples and the corpus becomes `@const`, including
associated constants: `pub const PI = …` becomes `pub @const PI = …`. There's no transition:
`const` is no longer read as a constant at all, and is an ordinary name.

---

## 2. Examples

### Tables computed while compiling

`example/tetris/main.som` writes out 28 rotation masks by hand. Instead, it writes the 7
pieces and lets the compiler rotate them:

```rust
/// Turns a 4x4 mask a quarter clockwise, inside its top-left `size`×`size` box.
fn rotate(mask: i32, size: i32) -> i32 {
    let mut turned = 0;
    for y in 0..size {
        for x in 0..size {
            if filled(mask, x, y) {
                // (x, y) moves to (size - 1 - y, x)
                turned = turned | (0x8000 >> (x * 4 + (size - 1 - y)));
            }
        }
    }
    turned
}

fn all_rotations() -> [i32; 28] {
    let bases = [0x0F00, 0x44C0, 0x4460, 0xCC00, 0x06C0, 0x0E40, 0x0C60]; // I J L O S T Z
    let boxes = [4, 3, 3, 2, 3, 3, 3];
    let mut masks = [0; 28];
    for k in 0..7 {
        let mut mask = bases[k];
        for r in 0..4 {
            masks[k * 4 + r] = mask;
            mask = rotate(mask, boxes[k]);
        }
    }
    masks
}

/// Worked out while compiling: the program only has the 28 numbers.
@const SHAPES: [i32; 28] = all_rotations();
```

This gives exactly the hand-written table (checked). `rotate` is an ordinary function, and
the game could call it at run time as well.

### A value worked out in place

```rust
fn crc32(bytes: &[u8]) -> u32 {
    let table = @const crc_table();      // [u32; 256], baked into the program
    …
}
```

### Unrolled loops

```rust
impl<N: usize> Vector<N, f32> {
    pub prop magnitude {
        let mut sum = 0.0;
        @for i in 0..N { sum = sum + .components[i] * .components[i]; }
        f32::sqrt(sum)
    }
}
```

Each copy has `i` as a known number, so `.components[i]` needs nothing from the prover.

### Branches only some types have

```rust
impl<N: usize, T: Copy> Vector<N, T> {
    @if N >= 3 {
        pub prop z = .components[2];
    }
}
```

For `Vector<2, _>` there is no `z` at all. This replaces the separate `impl Vector<2, T>`,
`impl Vector<3, T>` and `impl Vector<4, T>` blocks. Item-level `@if` like this comes in phase
3, see the [plan](#7-the-plan).

### Generating declarations

```rust
// std/hash.som: ten impls become one loop.
@for T in [u8, u16, u32, usize, i8, i16, i32, i64, isize] {
    impl Hash for T {
        fn hash(&self, hasher: &mut Hasher) { hasher.write(*self as u64); }
    }
}

// An enum's index and name, with nothing to keep up to date by hand.
impl Kind {
    pub fn index(self) -> usize = match self {
        @for (i, v) in @variants(Kind) { v => i, }
    };
}

// Field-by-field equality for any struct.
pub fn fields_eq<T>(a: &T, b: &T) -> bool {
    @for f in @fields(T) {
        if a.@field(f) != b.@field(f) { return false; }
    }
    true
}
```

---

## 3. How the compiler does it today

The relevant parts as they are now:

| Pass | What it does with constants |
|---|---|
| `1-lexer` | `@name` lexes as `TokenKind::Builtin`. `BUILTINS` lists them, for the parser and the editor |
| `2-parser` | `expr.rs` turns the `Builtin` token into `@unreachable`. `item.rs` reads `@where` |
| `3-resolve` | `check_const_expr` allows only literals, operators, casts, constants, arrays and structs; anything else is `ConstNotKnown`. `check_consts` finds `ConstCycle` |
| `4-typeck` | `const_number` works a constant out from its AST, for array lengths |
| `5-prove` | `const_int` does the same again, for facts |
| `6-lower` | A constant has no value of its own: each use lowers its AST expression again, in place |
| `9-optimize` | `fold.rs` folds MIR constants, wrapping like the C backend (`-fwrapv`). `shake.rs` drops what nothing uses |
| `10-codegen` | MIR to C. The C compiler lays out structs |

Two things follow from this:

1. **A constant's value is never stored anywhere.** It's an AST expression copied to each use.
   That's fine for `W * H` but wrong for a call: it would run once per use, at run time.
2. **Types need numbers before MIR exists.** `[i32; COLS * ROWS]` is worked out in typeck,
   long before there's any MIR to run. A constant computed by a function can't be known there
   without running code in the middle of type checking.

---

## 4. Design

### 4.1 Initializers: run code on the MIR that's already there

Each `@const` that isn't plain arithmetic, and each `@const` expression, is lowered into an
**initializer**: a MIR function with no parameters whose body computes the value. Wherever the
value is used, lowering writes a placeholder:

```rust
ConstKind::Pending(InitId)
```

From there:

1. `7-borrowcheck` checks initializers like any other function, so code run while compiling
   follows the same rules as code run later.
2. A new pass, `som_comptime`, runs each initializer once in a **MIR interpreter**, then
   replaces every `Pending(id)` with the value it got.
3. `shake.rs` drops the initializers, along with any function only they called.
   `all_rotations` and `rotate` never make it into the C.

The pipeline becomes:

```
lex → parse → resolve → typeck → prove → lower → borrowcheck → comptime → optimize → codegen
```

Running after the borrow checker means the interpreter only ever sees code that has passed
every check. It never has to deal with a broken program.

Lowering is monomorphized, so an `@const` inside a generic function is run once for each
instance, and `@const f(N)` uses that instance's `N`.

Plain constants (`@const COLS = 10;`) keep working as they do now. `const_number` and
`const_int` still handle them, and lowering still inlines them. Only constants that need code
run become initializers.

### 4.2 The interpreter

`som_comptime` gets a small interpreter over MIR:

```rust
enum Value {
    Int(i128, IntTy),       // wraps to the type's width, like -fwrapv
    Float(f64, FloatTy),
    Bool(bool),
    Unit,
    Str(Rc<str>),
    Func(FuncId),
    Aggregate { variant: Option<usize>, fields: Vec<Value> },  // struct, enum, array, optional
    Pointer { alloc: AllocId, offset: usize },                 // into its own heap
    Null,
}
```

- **Places** (`Projection`s) are paths into a frame's locals or into an allocation. References
  are places.
- **Intrinsics.** `Resize`, `Free`, `Read`, `Write`, `Offset`, `View` and `IsNull` work on an
  interpreter-owned heap: `Vec<Option<Vec<Value>>>`, one entry per allocation. This means
  `List`, `Map` and `string` work while compiling as long as they don't end up in the result
  (see 4.3).
- **Arithmetic** matches what the program would do at run time: integers wrap to their width,
  floats are IEEE `f64`/`f32`. `usize` and `isize` take the target's width, which for now is
  the host's, the same as the C backend.

Things that stop evaluation, each with its own error:

| Error | When |
|---|---|
| `ConstCallsExtern` | It calls an `extern "c"` function. Later, an allowlist of pure ones (`sqrt`, `sin`, …) done in Rust |
| `ConstReachedUnreachable` | It reaches `@unreachable` |
| `ConstDividedByZero` | It divides by zero, or computes `MIN / -1` |
| `ConstOutOfBounds` | It reads past an allocation, which only unchecked pointer code can do |
| `ConstTooLong` | It runs past a step limit (default 10 million MIR statements, `--const-steps` to change) |

Each error shows the **call stack while compiling**, using the spans MIR statements already
carry:

```
error: this constant reached `@unreachable`
  --> tetris/main.som:52:9
   |
52 |         @unreachable
   |         ^^^^^^^^^^^^ reached here
   = while working out `rotate(...)`, called at tetris/main.som:61:20
   = while working out `SHAPES`, at tetris/main.som:67:1
```

### 4.3 Baking the result into the program

What an initializer returns has to make sense in the compiled program, so it has to be
**free of memory**:

- ints, floats, bools, `()`
- `string`s with the literal count (`usize::MAX`), already part of the program and never freed
- arrays, structs, enums and optionals of the above

A result holding a pointer, a reference, an `Alloc` or a `List` is a `ConstHoldsMemory` error
that names the field: "`SHAPES.items` points to memory that only exists while compiling".
A `string` built while compiling is turned into a literal, because its bytes are known.

MIR gets one new constant:

```rust
ConstKind::Aggregate { variant: Option<usize>, fields: Vec<Const> }
```

Codegen writes small aggregates as C compound literals. Arrays over a size threshold become a
single `static const` global that each use reads from, so a 256-entry table isn't repeated at
every use.

### 4.4 Constants used in types (later)

In phase 1, a constant that needs code run can't be used where typeck or the prover need its
number: array lengths, value generic arguments, and the prover's facts. That's a
`ConstNeededTooEarly` error: "`N` is worked out by running code, which happens after types are
checked; write it as a number or as arithmetic of constants".

Lifting this means evaluating **on demand during typeck**: when `const_number` hits a computed
constant, type-check its initializer and what it calls, lower just those, and run them. The
`ConstCycle` check extends to this, so a constant whose type depends on itself is still an
error. It's how Zig works, and it's real work, so it waits until phase 4.

### 4.5 `@for` and `@if` inside function bodies

The body of an `@for` is **checked once per iteration**, with the loop variable bound as a
statically known value, the way a generic body is checked per type argument. Lowering unrolls
it. Because each copy knows `i`:

- `.components[i]` with `i` known to be below `N` needs no `@where` and no check
- each copy can have different types (needed for `@fields(T)`, phase 3)

`@if` type-checks only the branch its condition picks. The other branch is parsed and
resolved, but never type-checked or lowered. This is what lets `@if N >= 3 { .components[2] }`
compile for `N = 2`.

Phase 2 limit: the iterable and the condition can't depend on a generic parameter of the
function they're in. Allowing that means checking generic code once per instance (phase 3).

### 4.6 The big question: checking generics once vs per instance

Som checks a generic function once, against its bounds, like Rust does. Reflection
(`@fields(T)`, `@variants(E)`) and `@if N >= 3` in generic code don't fit that: what the body
contains depends on `T` and `N`.

Two ways to do it:

1. **Check per instance, only where it's needed.** A generic item that uses `@for`/`@if`
   over its own parameters, or reflection on them, is checked again for each instance it's used
   with. Errors name the instance ("in `Vector<2, f32>`"). Every other generic stays checked
   once. *Recommended*: the cost is paid only by code that asks for it.
2. **Bound the reflection.** `T: struct` could make `@fields(T)` available, with field types
   only known as `type`. This stays checked once but can do far less.

Phase 3 depends on this choice.

### 4.7 TODO: checking generics per instance

Decided: option 1 of [4.6](#46-the-big-question-checking-generics-once-vs-per-instance),
checking per instance only the generic code that needs it. Not started. The plan, in two
passes:

**Pass 1: function bodies.** `@if N >= 3 { … }` and `@if N == 2 { … }` in generic function
and method bodies, including branches that only type-check or prove for some `N`; `@for` over
a generic parameter where copies differ; and what step 14's reflection (`@fields(T)`) needs.

1. *Typeck finds them.* A generic function's body is checked as now. If it meets an `@if`
   or `@for` (later, reflection) that depends on the function's own generic parameters, what
   it found for that body is thrown away, its errors too, and the function is marked *per
   instance*. Other generic code stays checked once.
2. *Lowering asks for each instance.* Lowering already finds the instances (`describe<3>`,
   `Vector<2, f32>`), walking from `main`. For a per-instance function it asks for that
   instance to be checked: the body checked again with its parameters bound (`N` is
   `Ty::Value(3)`, `T` is `f32`). The check starts from lowering's own type table, so its
   `TypeId`s are lowering's (the table only grows, so lowering then takes it over). It gives a
   `Typed` for that body, where `@if N >= 3` is a known `true` or `false`.
3. *The prover runs per instance too*, with `N` a number, not `Place::Len(N)`: `.components[2]`
   under `@if N >= 3` is proven.
4. *Lowering builds the body from the instance's `Typed`.* `FnBuilder` gets a `typed` of its
   own instead of `self.l.typed` (about 100 places), the global one for everything else.
5. *The driver says which instance* an error is in: "in `describe<2>`: …". Instance errors
   are collected while lowering and reported before the borrow checker and codegen.
6. *The borrow checker* already works on per-instance MIR; `lower_generic`'s stand-ins skip
   per-instance functions.

Stages, the corpus passing after each: instance checking in typeck; lowering wired up, for
`@if` over a value parameter; the prover per instance; errors naming the instance.

**Pass 2: members only some instances have.** `@if N >= 3 { pub prop z = … }` among an
impl's members. What a type's members are then depends on the instance, so resolve, method
lookup and the LSP's completion each need one to answer. After pass 1 works.

### 4.8 Other things left for later

- **Where a failure is, when the program runs.** While compiling, an error in the standard
  library is said where the program called into it. When the program runs, `assert`'s
  `@fail` says `builtins.som:… in assert`. Planned: standard-library functions with `@fail` or
  `@unreachable` in them take the caller's `Location` as a hidden last parameter, passed on
  by any that are themselves such functions (Rust's `#[track_caller]`). `Location` would be
  `Copy`, its `file` and `function` `cstr`s.
- **Names made from an element**: `@for N in 1..4 { fn twice_N() … }` writes `twice_N` four
  times over. `@ident("twice_{N}")` (step 14), or another spelling.
- **The prover and arithmetic of two unknowns**: `.components[N - 1 - i]` in `@for i in 0..N`
  isn't proven, the prover's facts being one unknown and a constant. Proving each unrolled
  copy with `i` known would do it.
- **A computed constant in a type**: `[i32; four()]` is `ConstNeededTooEarly`. Typeck could
  run twice, the constants it needed worked out in between (cheaper than evaluating on demand,
  step 15).
- **The prover doesn't check a `@const`'s value**: `@where` conditions on calls made while
  compiling aren't proven; dividing by zero is caught as it runs.
- **The editor runs constants on every check**: one that runs for the step limit (10 million
  steps) takes a few seconds in a debug build. The editor could use a lower limit.
- **Types on functions** (from the same discussion, not compile-time code): no `->` meaning
  `()` rather than inferring the return type, and properties with a `: type`, like Rust.

---

## 5. Built-ins

New entries in `som_lexer::BUILTINS`, so the editor offers them:

| Built-in | Gives | Phase |
|---|---|---|
| `@const` | See above | 1 |
| `@for`, `@if` | See above | 2 |
| `@fail("why")` | Stops with a message: when the program runs, it says what failed and where; in a `@const`, an error while compiling (`ConstFailed`). Stands for a value of any type, like `@unreachable` | 1 |
| `@on fail` | On a line above a `fn`: called with a `&Failure` (`message`, `at: Location`, `unreachable`) when the program fails, at `@fail` or `@unreachable`. The program's own listeners are called, in order, instead of the standard library's `print_failure`; it can call that itself. Not called while compiling, where a failure is an error | 1 |
| `@embed("path")` | A file's bytes (`[u8; _]`) or text (`string`), relative to the source file | 1 |
| `@variants(E)` | An enum's variants, each with `.name` and `.index` | 3 |
| `@fields(T)` | A struct's fields, each with `.name` and `.type` | 3 |
| `value.@field(f)` | That field of `value` | 3 |
| `@ident("xy")` | A name, made from text | 3 |
| `@size_of(T)` | See below | 3 |

`assert(ok, "why")` isn't a built-in: it's an ordinary function in the standard library,
built on `@fail`. Called when the program runs, it checks then; called with `@`
(`@assert(SIZE % 8 == 0, "…");`, which can stand on its own at the top level), it checks
while compiling. Anyone can write their own checks the same way. While compiling, an
error in the standard library is said where the program called into it, not inside it.

`@size_of` needs care: the C compiler lays out structs, not som. The interpreter already works
out C's layout for the memory it allocates (`8-comptime/src/memory.rs`), mirroring codegen's
struct definitions, so `@size_of` can use that.

---

## 6. Errors, all together

| Kind | Pass | Phase |
|---|---|---|
| `ConstNotKnown` (exists, now also for `@const`, `@for`, `@if`) | resolve | 1 |
| `ConstCycle` (exists) | resolve | 1 |
| `ConstCallsExtern` | comptime | 1 |
| `ConstReachedUnreachable` | comptime | 1 |
| `ConstDividedByZero` | comptime | 1 |
| `ConstOutOfBounds` | comptime | 1 |
| `ConstTooLong` | comptime | 1 |
| `ConstHoldsMemory` | comptime | 1 |
| `ConstNeededTooEarly` | typeck | 1, gone in 4 |
| `ConstFailed` | comptime | 1 |

---

## 7. The plan

Section 8 breaks these phases into small steps, each with its own test.

**Phase 1: run code for values.**

0. *Parser:* `@const` declarations at item level, in `impl` blocks and in function bodies.
   `const` is still accepted, with a warning and a fix. Migrate `std/`, `example/` and
   `tests/corpus/` in the same change.
1. *Resolve:* `check_const_expr` allows calls, method calls, blocks, `if`, `match` and loops.
   It still reports locals of a surrounding function (`ConstNotKnown`) and still finds
   `ConstCycle`, now also through calls.
2. *Parser/lexer:* `@const` as a prefix expression, plus `@assert` and `@embed`.
3. *Typeck:* `const_number` returns `None` for computed constants. Where a number was required,
   report `ConstNeededTooEarly`.
4. *Lower:* computed constants and `@const` sites become initializers and `Pending` operands.
5. *New crate* `crates/compiler/8-comptime` (renumbering `optimize` and `codegen`): the
   interpreter, baking, and the errors above.
6. *MIR/codegen:* `ConstKind::Aggregate` and `static const` globals for big arrays.

   Done when `tests/corpus/constants/not_known_while_compiling.som`, rewritten with `@const`,
   changes from `expect: error ConstNotKnown` to `expect: exit 60`, and tetris uses `SHAPES`.

**Phase 2: `@for` and `@if` in function bodies,** over static values that don't depend on
the function's own generic parameters.

**Phase 3: generating items and reflection.** Item-level `@for`/`@if`, `@ident`, `@fields`,
`@variants`, types as values (`@for T in [u8, u16]`). Needs the decision in 4.6.

**Phase 4: computed constants in types,** evaluated on demand during typeck (4.4).

---

## 8. Steps

The phases broken into small steps. Each one needs exactly one thing the step before it
didn't, and each becomes a test in `tests/corpus/const_eval/` that should pass when that
step is done. Build them in this order.

### Phase 1

**Step 0. Constant arithmetic, spelled `@const`.** `step_00_arithmetic.som`, `exit 200`
```rust
@const COLS = 10;
@const ROWS = 20;
@const CELLS = COLS * ROWS;
```
*Needs:* the parser to accept `@const` declarations, and `std/`, `example/` and
`tests/corpus/` migrated from `const`. No interpreter yet. This is a good first commit on
its own. **Done.**

**Step 1. Call a simple function.** `step_01_call.som`, `exit 4`
```rust
fn kilobytes(n: i32) -> i32 = n * 1024;
@const BUFFER = kilobytes(4);
fn main() -> i32 = BUFFER / 1024;
```
*Needs:* calls allowed in `check_const_expr`, initializers and `Pending` in lowering, the
`som_comptime` crate with call frames, integer arithmetic and `return`, and baking a single
number. This step runs the whole pipeline end to end. **Done.**

**Step 2. Branches and recursion.** `step_02_recursion.som`
```rust
fn fib(n: i32) -> i32 = n if n < 2 else fib(n - 1) + fib(n - 2);
@const FIB_20 = fib(20);
```
*Needs:* the interpreter to follow branches and recursive calls, and the step limit
(`ConstTooLong`). **Done.**

**Step 3. Loops, changing locals, and an array as the result.** `step_03_array.som`
```rust
fn squares() -> [i32; 10] {
    let mut table = [0; 10];
    for i in 0..10 { table[i] = i * i; }
    table
}
@const SQUARES = squares();
```
*Needs:* places and indexing in the interpreter, range iterators (`IntoIterator`/`next`
calls), and `ConstKind::Aggregate` in MIR and codegen. **Done.**

**Step 4. Structs, enums and `match`.** `step_04_structs.som`
```rust
enum Channel { Red, Green, Blue }
struct Rgb { r: u8, g: u8, b: u8 }

fn pure(c: Channel) -> Rgb = match c {
    Red   => Rgb { r: 255, g: 0, b: 0 },
    Green => Rgb { r: 0, g: 255, b: 0 },
    Blue  => Rgb { r: 0, g: 0, b: 255 },
};
@const RED = pure(Channel::Red);
```
*Needs:* variants and `match` (a switch on the variant) in the interpreter, and struct
aggregates in codegen. **Done.**

**Step 5. Heap memory while running, with a result that has none.** `step_05_heap.som`
```rust
fn first_primes() -> [i32; 8] {
    let mut found: List<i32> = List::empty();
    let mut n = 2;
    while found.len < 8 {
        let mut prime = true;
        for p in &found { if p != 0 && n % p == 0 { prime = false; } }
        if prime { found.push(n); }
        n = n + 1;
    }
    let mut table = [0; 8];
    for i in 0..8 { table[i] = *found.at(i); }
    table
}
@const PRIMES = first_primes();
```
*Needs:* the interpreter heap for the `Alloc` intrinsics, references, and `Drop`. Returning
`found` itself is `ConstHoldsMemory` (`keeps_a_list.som`). **Done.**

**Step 6. Text built while compiling.** `step_06_text.som`
```rust
@const MAJOR = 0;
@const MINOR = 3;
fn banner() -> string = "som v{MAJOR}.{MINOR}";
@const BANNER = banner();
```
*Needs:* `Display` and `display_len` writing through raw `*u8` pointers, and turning the
built `string` into a literal (count `usize::MAX`) in the program. **Done.**

**Step 7. Constants inside impl blocks and functions.** `step_07_scopes.som`
```rust
impl f32 {
    pub @const TAU = f32::PI * 2.0;
}

fn wrap(i: usize) -> usize {
    @const SIZE = kilobytes(1) as usize;
    i % SIZE if SIZE != 0 else 0
}
```
*Needs:* `@const` declarations in `impl` blocks and function bodies. In a body, the parser
tells a declaration (a name, then `:` or `=`) from an expression. **Done.**

**Step 8. `@const` as an expression.** `step_08_in_place.som`
```rust
fn is_prime_small(n: i32) -> bool {
    for p in &@const first_primes() { if p == n { return true; } }
    false
}
```
*Needs:* `@const` as a prefix expression, initializers created at an expression rather than
for a name, and `ConstNotKnown` when it names a local such as `n` (`local_named.som`). **Done.**

**Step 9. Checks and files while compiling.** `step_09_assert_embed.som`
```rust
@assert(PRIMES[0] == 2, "the first prime is 2");
@const HELP: string = @embed("help.txt");
```
*Needs:* `@fail` and the standard library's `assert` (**done**: `const_eval/assert*.som`,
`fail_in_a_function.som`), and the `@embed` built-in, with paths relative to the source
file. Phase 1 is done.

### Phase 2

**Step 10. `@const` once per generic instance.** `step_10_per_instance.som`
```rust
fn identity<N: usize>() -> [[i32; N]; N] = @const make_identity<N>();
```
*Needs:* `N` to count as statically known in monomorphized code, with one initializer per
instance. (This only needs phase 1's machinery, but it's the first step that touches
generics, so it opens phase 2.) **Done**: lowering keys an initializer by its constant and
the function instance it's in, and builds it with that instance's arguments.

**Step 11. `@for` and `@if` over fixed values.** `step_11_unroll.som`
```rust
fn sum4(v: [i32; 4]) -> i32 {
    let mut s = 0;
    @for i in 0..4 { s = s + v[i]; }
    s
}

@const DEBUG = false;
fn log(msg: string) { @if DEBUG { print(msg); } }
```
*Needs:* the body checked once per iteration with `i` known, the loop unrolled in lowering,
and the untaken `@if` branch never type-checked. Phase 2 is done. **Done**, with the body checked
once (each copy has the same types until phase 3): `@for` loops over a range of known numbers
(including a generic function's `N`, settled per instance in lowering) or an array written
out, and lowering unrolls it; `@if` and `else @if` take their condition from literals and
constants, and only the branch picked is checked and lowered.

### Phase 3

**Step 12. `@for` over a generic parameter.** `step_12_generic_for.som`
```rust
impl<N: usize> Vector<N, f32> {
    pub prop magnitude {
        let mut sum = 0.0;
        @for i in 0..N { sum = sum + .components[i] * .components[i]; }
        f32::sqrt(sum)
    }
}
```
*Needs:* nothing beyond step 11. **Done**: the body is checked once, its types the same in
every copy, and lowering unrolls it for each instance's `N`. The decision in
[4.6](#46-the-big-question-checking-generics-once-vs-per-instance) is only needed once copies
can differ in type: `@if N >= 3`, and reflection (step 13 on). The prover checks the body once
too, for every `N`: `.components[i]` is fine, but `.components[N - 1 - i]` isn't proven, its
facts being one unknown and a constant. Proving each copy with `i` known would fix that.

**Step 13. Generating items.** `step_13_items.som`
```rust
impl<N: usize, T: Copy> Vector<N, T> {
    @if N >= 3 { pub prop z = .components[2]; }
}

@for T in [u8, u16, u32, i32, i64] {
    impl Hash for T { fn hash(&self, h: &mut Hasher) { h.write(*self as u64); } }
}
```
*Needs:* `@for` and `@if` at item level, types as values, and the generated items registered
with resolve after expansion.

`@for` around items is **done**, written out before parsing: each copy of the body's tokens,
with the name written as that element (a type, or a number of a range), is put where the
`@for` was and parsed like anything else. Its list has to be written out, `[u8, u16]` or
`0..4`, since nothing is worked out yet. `std/hash.som` uses it. Not done: `@if N >= 3` in a
generic impl, which needs [4.6](#46-the-big-question-checking-generics-once-vs-per-instance);
and names made from the element (`fn twice_{N}`), which is `@ident`.

**Step 14. Reflection.** `step_14_reflection.som`
```rust
impl Kind {
    pub fn index(self) -> usize = match self {
        @for (i, v) in @variants(Kind) { v => i, }
    };
}

pub fn fields_eq<T>(a: &T, b: &T) -> bool {
    @for f in @fields(T) { if a.@field(f) != b.@field(f) { return false; } }
    true
}
```
*Needs:* `@variants`, `@fields`, `.@field(f)`, and `@for` producing `match` arms. `@ident`
can come here too, for swizzles. Phase 3 is done.

Written `@for (i, v) in Kind`, not `@for (i, v) in @variants(Kind)`: going through a type
goes through its variants (and, for a struct, will go through its fields). **Done** for
`match` arms: the `@for` is kept, its body's place in the source and where its copies go,
until every module is loaded; then `Kind` is looked up, preferring the module it's written
in, and each copy's tokens are written with `v` as the variant's pattern (`Red`,
`Circle(_)`, `Rect { .. }`) and `i` as its index, parsed, and put among the arms.
Resolve and the type checker see the arms written out, so a `match` is still checked to cover
every variant. Not done: `@for` over a type among statements or items, structs' fields,
`v.name`, and anything in generic code (which needs 4.7).

### Phase 4

**Step 15. A computed constant as a type.** `step_15_in_types.som`
```rust
@const SIZE = next_power_of_two(100);
struct Ring { slots: [i32; SIZE] }
```
*Needs:* evaluating on demand during typeck, with cycle detection through types. Until this
step, this test expects `error ConstNeededTooEarly`, and the step flips it.

---

## 9. Error cases and other tests

All in `tests/corpus/const_eval/`, in the corpus's usual format (`// expect: exit N` /
`// expect: error Kind`), next to the step tests from section 8. Some overlap with the steps;
they're listed here so every error kind has a test:

| File | Expect |
|---|---|
| `calls_a_function.som`: `@const W = width();` | `exit 60` |
| `table.som`: `@const SHAPES = all_rotations();`, sums them | `exit …` |
| `with_a_list.som`: builds a `List` while compiling, returns an array | `exit …` |
| `string_built.som`: concatenates while compiling | `exit …` |
| `in_place.som`: `@const` in a function body | `exit …` |
| `per_instance.som`: `@const f(N)` in a generic | `exit …` |
| `local_named.som`: `@const f(n)` with `n` a local | `error ConstNotKnown` |
| `calls_c.som`: `@const X = c::rand();` | `error ConstCallsExtern` |
| `reaches_unreachable.som` | `error ConstReachedUnreachable` |
| `divides_by_zero.som` | `error ConstDividedByZero` |
| `forever.som`: `loop {}` | `error ConstTooLong` |
| `keeps_a_list.som`: returns a `List` | `error ConstHoldsMemory` |
| `array_length.som`: `[i32; computed()]` | `error ConstNeededTooEarly` |
| `calls_itself.som`: `@const A = f(); fn f() = A;` | `error ConstCycle` |
| `assert.som`: `@assert(SIZE % 8 == 0, "…")` with `SIZE` 12 | `error ConstFailed` |
| `wraps.som`: `i8` overflow while compiling equals run time | `exit …` |

There's also one property worth testing outside the corpus: for any function in the corpus,
`@const f(args)` and `f(args)` give the same value. That catches the interpreter and the C
backend drifting apart.

---

## 10. Not in scope

- Running code while compiling that has effects: printing, files (apart from `@embed`),
  the network, the clock. Results must depend only on the source.
- Generating code from text (Jai's `#insert "…"`). `@ident` covers names; everything else is
  written as som.
- Caching results between builds.

# Implementing Closures: Functions That Remember

*A comprehensive guide to adding closures to a compiler, exploring the theory, history, and techniques that make first-class functions work.*

---

## Table of Contents

1. [Introduction: What Makes Closures Special](#introduction-what-makes-closures-special)
2. [The Funarg Problem: A Historical Perspective](#the-funarg-problem-a-historical-perspective)
3. [Closure Semantics: Capturing the Environment](#closure-semantics-capturing-the-environment)
4. [Upvar Analysis: Knowing What to Capture](#upvar-analysis-knowing-what-to-capture)
5. [Capture Modes: By Value, By Reference, By Move](#capture-modes-by-value-by-reference-by-move)
6. [Closure Conversion: Implementation Strategies](#closure-conversion-implementation-strategies)
7. [Boxing vs Unboxed Closures](#boxing-vs-unboxed-closures)
8. [Rust's Fn Traits: A Case Study](#rusts-fn-traits-a-case-study)
9. [Type Systems for Closures](#type-systems-for-closures)
10. [Closures and Ownership](#closures-and-ownership)
11. [Optimization Techniques](#optimization-techniques)
12. [Implementation Walkthrough](#implementation-walkthrough)
13. [Testing Strategy](#testing-strategy)
14. [Further Reading](#further-reading)

---

## Introduction: What Makes Closures Special

A closure is a function that "closes over" its environment—it captures variables from the scope where it was defined and carries them along wherever it goes.

```rust
fn make_adder(x: i32) -> impl Fn(i32) -> i32 {
    |y| x + y  // This closure captures `x`
}

let add_five = make_adder(5);
println!("{}", add_five(3));  // Prints 8
```

This seems simple, but it raises profound questions:

- Where does `x` live after `make_adder` returns?
- How does the closure "remember" its value?
- What if `x` was mutable? Can the closure modify it?
- What if multiple closures capture the same variable?
- How do we represent this in machine code?

These questions plagued language designers for decades. The solutions they found shaped modern programming languages and gave us concepts like garbage collection, ownership, and effect systems.

### Why Closures Matter

Closures aren't just a convenience—they're a fundamental building block:

**Higher-order functions**: Map, filter, reduce all take closures.
```rust
let squares: Vec<_> = numbers.iter().map(|x| x * x).collect();
```

**Callbacks and event handling**: GUIs, async code, and reactive programming rely on closures.
```rust
button.on_click(|event| {
    counter += 1;  // Captures `counter` from enclosing scope
    update_display();
});
```

**Deferred computation**: Closures let you package up work to do later.
```rust
let expensive = || {
    // Only computed when called
    heavy_computation()
};
```

**Encapsulation without classes**: Closures can create private state.
```javascript
function makeCounter() {
    let count = 0;
    return {
        increment: () => ++count,
        get: () => count
    };
}
```

Understanding closures deeply means understanding core concepts in programming language theory: scope, lifetime, memory management, and the relationship between functions and data.

---

## The Funarg Problem: A Historical Perspective

The story of closures begins with one of the oldest problems in programming language design: the *funarg problem*.

### LISP and the Birth of Closures (1958-1960)

John McCarthy's LISP was the first language with first-class functions—functions could be passed as arguments and returned as values. But there was a problem.

Consider this LISP code (in modern notation):

```lisp
(defun make-adder (x)
  (lambda (y) (+ x y)))

(setq add-five (make-adder 5))
(funcall add-five 3)  ; Should return 8
```

When `make-adder` returns, what happens to `x`? In most languages of the era, local variables lived on *the stack*. When a function returned, its stack frame was destroyed, and its local variables ceased to exist.

But `add-five` still needs `x`! If `x` was on the stack and the stack frame is gone, we have a *dangling reference*—a pointer to memory that's been reclaimed.

This is the **funarg problem** (function argument problem). It has two variants:

### The Downward Funarg Problem

When you pass a function *down* into another function:

```
function outer() {
    let x = 10;
    inner(lambda() { return x; });  // Pass closure down
}

function inner(f) {
    return f();  // Call it within inner
}
```

This is relatively easy to handle. When `inner` calls `f`, we're still inside `outer`'s dynamic extent—`outer`'s stack frame is still there, underneath `inner`'s. The variable `x` is still valid.

The downward funarg problem was solved early. ALGOL 60 handled it by passing a *static link*—a pointer to the enclosing stack frame—along with the function.

### The Upward Funarg Problem

When you return a function *up* out of its defining scope:

```
function outer() {
    let x = 10;
    return lambda() { return x; };  // Return closure up
}

let f = outer();  // outer returns, its stack frame is gone
f();              // But we still need x!
```

This is the hard one. When `outer` returns, its stack frame is popped. But the returned lambda still references `x`. Where does `x` live now?

### The Solutions

Different languages solved this differently:

**LISP (dynamic scoping initially)**: Early LISP used *dynamic scoping*, where variable lookup happened at runtime based on the call stack. This avoided the problem but had its own issues (variables could accidentally shadow each other in confusing ways).

**Scheme (1975)**: Introduced *lexical scoping* with proper closures. Variables are captured at definition time, not call time. To make this work, captured variables are moved to the *heap* when necessary. This is called *closure conversion*.

**ML (1973)**: Also used lexical scoping with heap-allocated closures. ML was designed around the idea that functions are values.

**C (1972)**: Took the easy route—no nested functions, no closures. (GCC added nested functions as an extension, but returning them is undefined behavior.)

**Smalltalk (1972-1980)**: Blocks (closures) capture their enclosing context. Smalltalk's garbage collector manages their lifetime.

**Modern languages**: Most use heap allocation with garbage collection (JavaScript, Python, Ruby, Go, Java lambdas) or ownership/lifetime tracking (Rust, C++).

### Why This History Matters

Understanding the funarg problem helps you understand *why* closures are implemented the way they are:

1. **Why some languages don't have closures**: They wanted simple stack-based memory management.

2. **Why closures often involve heap allocation**: Captured variables may outlive their original scope.

3. **Why Rust's closures are complex**: Rust wants closures *without* garbage collection, which requires sophisticated static analysis.

4. **Why "closure" means "closes over"**: The lambda expression "closes over" free variables from its environment, creating a self-contained unit.

---

## Closure Semantics: Capturing the Environment

When a closure captures a variable, what exactly does it capture? This is a semantic choice with significant implications.

### Free Variables and Bound Variables

In any expression, variables are either *free* or *bound*:

```rust
|x| x + y
```

- `x` is *bound*—it's a parameter of the lambda
- `y` is *free*—it comes from somewhere else

Closures exist because of free variables. If a lambda has no free variables, it's just a regular function—it doesn't need to capture anything.

```rust
let f = |x| x + 1;        // No captures, could be a plain function
let g = |x| x + offset;   // Captures `offset`, needs to be a closure
```

### What Is an "Environment"?

The environment is the mapping from variable names to their values (or locations) at the point where the closure is created.

```rust
let a = 1;
let b = 2;
let c = 3;

let f = |x| a + b + x;  // Environment: {a: 1, b: 2}
                        // Note: c is NOT captured
```

A closure needs to store only the variables it actually uses. Capturing everything in scope would be wasteful (and in languages with ownership, would cause unnecessary moves or borrows).

### Capture Time vs Call Time

A crucial distinction: when are the values of captured variables determined?

**Capture time** (lexical scoping):
```javascript
let x = 1;
let f = () => x;
x = 2;
console.log(f());  // Prints 1 if captured by value at definition
                   // Prints 2 if captured by reference
```

**Call time** (dynamic scoping):
```
x = 1
f = lambda: x
x = 2
f()  # Always prints 2—looks up x when called
```

Almost all modern languages use capture-time semantics (lexical scoping). The only question is whether they capture *values* or *references*.

### Early Binding vs Late Binding

Related but distinct from scoping:

**Early binding**: The *meaning* of names is determined at compile time.
**Late binding**: The *meaning* of names is determined at runtime.

Closures with lexical scoping use early binding for name resolution (we know *which* `x` we mean) but may use either early or late binding for *values* (do we capture the value now, or the location?).

---

## Upvar Analysis: Knowing What to Capture

Before we can implement closures, we need to know which variables they capture. This is called *upvar analysis* or *free variable analysis*.

### The Algorithm

For each lambda expression:

1. Collect all variable references in the body
2. Remove parameter names (those are bound, not free)
3. Remove locally defined names (let bindings inside the lambda)
4. What remains are the free variables that must be captured

```rust
fn analyze_free_vars(lambda: &Lambda) -> HashSet<String> {
    let used = collect_var_uses(&lambda.body);
    let bound = lambda.parameters.clone();
    let local = collect_local_definitions(&lambda.body);

    used.difference(&bound).difference(&local).cloned().collect()
}
```

### Handling Nested Closures

Closures can be nested, and inner closures may reference variables from outer closures:

```rust
fn outer(a: i32) -> impl Fn() -> impl Fn() -> i32 {
    let b = 10;
    move || {
        let c = 20;
        move || {
            a + b + c  // Captures a, b from outer; c from middle
        }
    }
}
```

Each closure captures from its immediate environment. The middle closure captures `a` and `b`, and the inner closure captures `c` from middle and `a`, `b` (indirectly, through middle's captures).

This creates a chain of environments. Implementation options:

1. **Flat closures**: Each closure copies all variables it needs, including those from outer scopes
2. **Linked closures**: Each closure points to its parent's environment
3. **Hybrid**: Copy immediate captures, link to parent for older ones

### Mutation Complicates Everything

If captured variables can be mutated, analysis gets harder:

```javascript
function makeCounters() {
    let count = 0;
    return {
        increment: () => ++count,
        decrement: () => --count,
        get: () => count
    };
}
```

All three closures capture `count`, and two of them mutate it. They must all share the *same* storage for `count`, not independent copies.

This means:
1. We need to detect when captures are mutated
2. Mutated captures usually must be captured by reference
3. The storage must outlive all closures that share it

### When Captures Escape

A key question for memory management: does the closure escape its defining scope?

```rust
fn no_escape(data: &[i32]) {
    data.iter().for_each(|x| println!("{}", x));
    // Closure doesn't escape—created and used within this function
}

fn escapes() -> impl Fn() -> i32 {
    let x = 42;
    || x  // Closure escapes—returned to caller
}
```

Non-escaping closures are easier to optimize. Since they're used only locally, their captures don't need to outlive the current stack frame. Escaping closures are harder—their captures must be moved to longer-lived storage.

Rust's lifetime system makes this explicit. Other languages determine it at runtime (garbage collection) or don't distinguish (always heap-allocate captures).

---

## Capture Modes: By Value, By Reference, By Move

The most important semantic choice: how does a closure capture a variable?

### Capture by Value (Copy)

The closure gets its own copy of the variable's value:

```rust
let x = 42;
let f = || x;  // Captures a copy of x
let g = || x;  // Gets another copy

// x, f's copy, and g's copy are independent
```

**Advantages**:
- Simple semantics
- No sharing, no aliasing problems
- Closure is self-contained

**Disadvantages**:
- Can't mutate the original
- Expensive for large values
- Multiple closures can't share state

### Capture by Reference

The closure stores a reference to the variable:

```rust
let mut x = 0;
let mut f = || x += 1;  // Captures &mut x

f();
f();
println!("{}", x);  // Prints 2
```

**Advantages**:
- Can mutate the original
- No copying
- Multiple closures can share (with appropriate synchronization)

**Disadvantages**:
- Creates aliasing
- Lifetime of reference must not outlive original
- The upward funarg problem!

### Capture by Move

The closure takes ownership of the variable:

```rust
let s = String::from("hello");
let f = move || println!("{}", s);  // `s` moved into closure
// s is no longer accessible here
```

**Advantages**:
- Clear ownership semantics
- Closure owns its captures
- Can outlive defining scope without heap allocation

**Disadvantages**:
- Original variable is gone
- Can't be used for sharing

### Language Differences

**JavaScript**: Always by reference. Variables are in a heap-allocated scope.

```javascript
let x = 1;
let f = () => x;
x = 2;
f();  // Returns 2—captured by reference
```

**Python**: Reads by reference, but assignment creates new local. Use `nonlocal` to mutate.

```python
def outer():
    x = 1
    def f():
        nonlocal x
        x += 1
    return f
```

**C++**: You choose per-variable.

```cpp
int x = 1, y = 2;
auto f = [x, &y]() {  // x by value, y by reference
    // x is copied, y is referenced
};
```

**Rust**: Inferred from usage, or forced with `move`.

```rust
let s = String::from("hello");
let f = || println!("{}", s);       // Borrows s
let g = move || println!("{}", s);  // Moves s
```

**Java**: Captures must be "effectively final"—can't be mutated after capture. This sidesteps the whole problem.

```java
int x = 10;
// x = 20;  // Would make x non-effectively-final
Runnable r = () -> System.out.println(x);  // OK
```

### The Reference vs Value Tradeoff

The choice between reference and value capture often comes down to:

1. **Lifetime**: Will the closure outlive the original variable?
   - If yes, can't capture by reference (without heap allocation)
   - Capture by value or move instead

2. **Mutation**: Does the closure need to modify the variable?
   - If yes, must capture by reference (or move a mutable container)

3. **Sharing**: Do multiple closures access the same variable?
   - If yes, need reference capture
   - With mutation, also need synchronization

4. **Size**: Is the variable large?
   - If yes, copy is expensive
   - Consider reference or move

---

## Closure Conversion: Implementation Strategies

Now for the implementation. How do we compile closures to machine code?

### Strategy 1: Lambda Lifting

The simplest approach: convert closures to regular functions by making captured variables explicit parameters.

Before:
```rust
fn outer(x: i32) -> impl Fn(i32) -> i32 {
    |y| x + y
}
```

After lambda lifting:
```rust
fn closure_1(x: i32, y: i32) -> i32 {
    x + y
}

fn outer(x: i32) -> (fn(i32, i32) -> i32, i32) {
    (closure_1, x)  // Return function pointer and captured value
}
```

At call sites, we need to pass the captured values:
```rust
let (f, captured_x) = outer(5);
f(captured_x, 3);  // Equivalent to original
```

**Advantages**:
- Simple transformation
- No heap allocation
- Closures become regular functions

**Disadvantages**:
- Caller must know about captures (breaks abstraction)
- Doesn't match Fn trait interface
- Complex with multiple/nested closures

Lambda lifting is used in functional language compilers (like GHC for Haskell) combined with other optimizations. It's rarely the *only* strategy.

### Strategy 2: Closure Conversion (Environment Passing)

The classic approach: package the captured environment into a data structure and pass it with the function.

A closure becomes a pair:
```
closure = (function_pointer, environment_pointer)
```

Before:
```rust
fn outer(x: i32) -> impl Fn(i32) -> i32 {
    |y| x + y
}
```

After closure conversion:
```rust
// The environment is a struct with captured variables
struct Env_1 {
    x: i32,
}

// The function takes the environment as a hidden first parameter
fn closure_1(env: &Env_1, y: i32) -> i32 {
    env.x + y
}

fn outer(x: i32) -> Closure {
    let env = Env_1 { x };
    Closure {
        func: closure_1,
        env: Box::new(env),
    }
}
```

When calling:
```rust
// closure.call(arg) becomes:
(closure.func)(&closure.env, arg)
```

This is essentially what most languages do. The environment can be heap-allocated (for escaping closures) or stack-allocated (for non-escaping ones).

### Strategy 3: Defunctionalization

Replace function values with data, and function calls with pattern matching.

Before:
```rust
enum Closure {
    Adder { x: i32 },
    Multiplier { x: i32 },
}

fn apply(c: Closure, y: i32) -> i32 {
    match c {
        Closure::Adder { x } => x + y,
        Closure::Multiplier { x } => x * y,
    }
}
```

This eliminates function pointers entirely! All closures become data that a central `apply` function interprets.

**Advantages**:
- No function pointers
- Enables whole-program optimization
- Works in languages without first-class functions

**Disadvantages**:
- Requires whole-program analysis
- `apply` becomes huge in real programs
- Harder with higher-order functions

Defunctionalization is used in some functional language compilers and is the basis for continuation-passing style transformations.

### Strategy 4: Inline Everything

For non-escaping closures called immediately, just inline:

Before:
```rust
let x = 5;
let f = |y| x + y;
let result = f(3);
```

After inlining:
```rust
let x = 5;
let result = x + 3;
```

No closure needed! This is the best optimization when applicable.

### Choosing a Strategy

Most compilers use a combination:

1. **Inline** non-escaping closures called at known sites
2. **Stack-allocate** environments for non-escaping closures passed to other functions
3. **Heap-allocate** environments for escaping closures
4. **Lambda-lift** when it reduces allocations

---

## Boxing vs Unboxed Closures

### The Uniform Representation Problem

If closures have different environments, they have different sizes:

```rust
let f = |x| x + 1;           // No captures, size = 0
let g = |x| x + a;           // Captures a: i32, size = 4
let h = |x| x + a + b + c;   // Captures three i32s, size = 12
```

How do you store these in a collection? Or pass them to a function that accepts "any closure"?

```rust
fn apply_twice(f: ???, x: i32) -> i32 {
    f(f(x))
}
```

Two solutions: boxing or monomorphization.

### Boxed Closures (Dynamic Dispatch)

Store closures as trait objects with dynamic dispatch:

```rust
fn apply_twice(f: &dyn Fn(i32) -> i32, x: i32) -> i32 {
    f(f(x))
}

// or with ownership:
fn apply_twice(f: Box<dyn Fn(i32) -> i32>, x: i32) -> i32 {
    f(f(x))
}
```

How it works:
```
┌───────────────────┐
│  Closure Object   │
├───────────────────┤
│  data pointer  ───┼──► Environment { x: i32, y: i32, ... }
│  vtable pointer ──┼──► VTable { call: fn_ptr, drop: fn_ptr, ... }
└───────────────────┘
```

The vtable contains function pointers for calling the closure, dropping it, etc. This is the same mechanism as trait objects/interfaces.

**Advantages**:
- Uniform representation (fat pointer, always 2 words)
- Can store different closures in the same collection
- Works at runtime (no compile-time type resolution needed)

**Disadvantages**:
- Heap allocation for environment
- Indirect call through vtable
- Can't inline

### Unboxed Closures (Static Dispatch / Monomorphization)

Generate specialized code for each closure type:

```rust
fn apply_twice<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(f(x))
}
```

The compiler generates different versions of `apply_twice` for each `F` it's called with. Each closure has its own generated struct:

```rust
// For |x| x + 1
struct Closure_0;
impl Fn(i32) -> i32 for Closure_0 {
    fn call(&self, x: i32) -> i32 { x + 1 }
}

// For |x| x + a
struct Closure_1 { a: i32 }
impl Fn(i32) -> i32 for Closure_1 {
    fn call(&self, x: i32) -> i32 { x + self.a }
}
```

**Advantages**:
- No heap allocation (closure can live on stack)
- Direct function call (can be inlined)
- Zero runtime overhead

**Disadvantages**:
- Code bloat (many specialized versions)
- Compile-time cost
- Can't mix different closures in a collection (without boxing)

### The Rust Approach

Rust uses unboxed closures by default with optional boxing:

```rust
// Unboxed, monomorphized
fn map<F: Fn(i32) -> i32>(xs: Vec<i32>, f: F) -> Vec<i32> { ... }

// Boxed, dynamic dispatch
fn callbacks: Vec<Box<dyn Fn() -> ()>> = vec![];

// Borrowed trait object
fn apply(f: &dyn Fn(i32) -> i32, x: i32) -> i32 { f(x) }
```

Each closure is a unique anonymous type that implements `Fn`, `FnMut`, or `FnOnce`. You choose boxing explicitly when you need runtime flexibility.

### C++ Comparison

C++ lambdas are also unboxed. Each lambda has a unique anonymous type:

```cpp
auto f = [](int x) { return x + 1; };
auto g = [](int x) { return x + 1; };
// f and g have DIFFERENT types!
```

To store them uniformly, use `std::function` (which boxes):

```cpp
std::function<int(int)> f = [](int x) { return x + 1; };
std::function<int(int)> g = [a](int x) { return x + a; };
std::vector<std::function<int(int)>> funcs = {f, g};
```

### Java and Go

**Java**: Lambdas are instances of functional interfaces. They're boxed but heavily optimized.

```java
Function<Integer, Integer> f = x -> x + 1;
```

**Go**: Functions can be closures. The environment is heap-allocated. Go's escape analysis may stack-allocate non-escaping closures.

```go
func makeAdder(x int) func(int) int {
    return func(y int) int { return x + y }
}
```

---

## Rust's Fn Traits: A Case Study

Rust's closure traits are a masterclass in capturing closure semantics in a type system. Let's understand them deeply.

### The Three Traits

```rust
trait FnOnce<Args> {
    type Output;
    fn call_once(self, args: Args) -> Self::Output;
}

trait FnMut<Args>: FnOnce<Args> {
    fn call_mut(&mut self, args: Args) -> Self::Output;
}

trait Fn<Args>: FnMut<Args> {
    fn call(&self, args: Args) -> Self::Output;
}
```

The key difference is how `self` is taken:
- `FnOnce`: takes `self` by value (consumes the closure)
- `FnMut`: takes `&mut self` (can mutate captures)
- `Fn`: takes `&self` (immutable access to captures)

There's a subtyping relationship: `Fn: FnMut: FnOnce`. Any `Fn` can be used where `FnMut` is expected, and any `FnMut` can be used where `FnOnce` is expected.

### Why Three Traits?

Consider what closures can do with their captures:

**Read-only access** → `Fn`
```rust
let x = 42;
let f = || x;  // Just reads x
// f implements Fn—can be called many times
```

**Mutable access** → `FnMut`
```rust
let mut count = 0;
let mut f = || count += 1;  // Mutates count
// f implements FnMut—needs &mut self to call
```

**Move/consume captures** → `FnOnce`
```rust
let s = String::from("hello");
let f = || drop(s);  // Consumes s
// f implements FnOnce only—can only be called once
```

### Inference Rules

Rust infers the most permissive trait:

```rust
let s = String::from("hello");

// Only reads s: implements Fn (+ FnMut + FnOnce)
let f = || println!("{}", s);

// Mutates s: implements FnMut (+ FnOnce), not Fn
let mut f = || s.push_str("!");

// Consumes s: implements FnOnce only
let f = move || drop(s);
```

The `move` keyword affects *how* variables are captured (by move vs by reference), not *which trait* is implemented.

### Capture Analysis

Rust's compiler analyzes how each captured variable is used:

```rust
let a = String::from("a");  // Used by ref
let b = String::from("b");  // Used by mut ref
let c = String::from("c");  // Used by move

let mut f = || {
    println!("{}", a);  // Reads a
    b.push_str("x");    // Mutates b
    drop(c);            // Consumes c
};
```

Without `move`, Rust captures:
- `a` by `&String` (immutable borrow)
- `b` by `&mut String` (mutable borrow)
- `c` by `String` (moved—can't just borrow something you consume)

With `move`, everything is moved into the closure.

### The Desugaring

Each closure becomes an anonymous struct:

```rust
let x = 1;
let y = 2;
let f = |z| x + y + z;
```

Desugars to something like:

```rust
struct __Closure_f<'a> {
    x: &'a i32,
    y: &'a i32,
}

impl<'a> Fn<(i32,)> for __Closure_f<'a> {
    type Output = i32;

    fn call(&self, (z,): (i32,)) -> i32 {
        *self.x + *self.y + z
    }
}

// Also implements FnMut and FnOnce by forwarding to call
```

### Higher-Ranked Trait Bounds

Sometimes closure types need higher-ranked bounds:

```rust
fn apply<F>(f: F) -> i32
where
    F: for<'a> Fn(&'a str) -> i32,  // Works with ANY lifetime
{
    f("hello")
}
```

This says `F` must work with *any* lifetime `'a`, not a specific one. It's necessary when closures take references with different lifetimes across calls.

### Clone and Copy for Closures

Closures implement `Clone` or `Copy` if all their captures do:

```rust
let x = 42;  // i32: Copy
let f = || x;  // f: Copy

let s = String::from("hello");  // String: !Copy
let g = || s.len();  // g: !Copy (captures &String, which is Copy... wait)
// Actually g IS Copy because it captures a reference
```

The rules are subtle. The closure's "copyability" depends on how captures are stored, not the types of the original variables.

---

## Type Systems for Closures

How do type systems handle closures? This is surprisingly deep.

### The Typing Challenge

The type of a closure includes:
1. Parameter types
2. Return type
3. The types of captured variables
4. How variables are captured (by value, by ref, mutably)

Consider:
```rust
let x: i32 = 1;
let y: String = String::from("hello");
let f = |z: bool| if z { x } else { y.len() as i32 };
```

What is `f`'s type? Something like:
```
Closure<captures=(x: i32, y: &String), params=(bool,), returns=i32>
```

But we don't want to expose all this detail in function signatures!

### Approach 1: Existential Types

Hide the closure type behind an existential:

```rust
fn make_adder(x: i32) -> impl Fn(i32) -> i32 {
    |y| x + y
}
```

`impl Fn(i32) -> i32` means "some type that implements `Fn(i32) -> i32`." The caller doesn't know the concrete type.

**Advantages**: Clean signatures, abstraction.
**Disadvantages**: Can't name the type, limited in what you can do with it.

### Approach 2: Type Parameters

Use generics to accept any closure:

```rust
fn apply<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}
```

The concrete type flows through as a type parameter.

**Advantages**: Maximum flexibility, zero-cost abstraction.
**Disadvantages**: Each instantiation creates new code, types can become complex.

### Approach 3: Trait Objects

Erase the type behind a vtable:

```rust
fn apply(f: &dyn Fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}
```

**Advantages**: Uniform representation, works at runtime.
**Disadvantages**: Heap allocation, indirect calls.

### Recursive Closures

A closure that calls itself is tricky:

```rust
// Doesn't work—closure's type isn't known yet
let factorial = |n| if n <= 1 { 1 } else { n * factorial(n - 1) };
```

The problem: to type `factorial`, we need to know its type, but its type includes itself!

Solutions:

**Y combinator**:
```rust
fn y<T, R>(f: impl Fn(&dyn Fn(T) -> R, T) -> R) -> impl Fn(T) -> R {
    move |x| f(&|x| y(&f)(x), x)
}

let factorial = y(|f, n| if n <= 1 { 1 } else { n * f(n - 1) });
```

**Explicit recursion helper**:
```rust
fn fix<T, R>(f: impl Fn(&dyn Fn(T) -> R, T) -> R) -> impl Fn(T) -> R {
    struct Fix<F>(F);
    impl<T, R, F: Fn(&dyn Fn(T) -> R, T) -> R> Fn<(T,)> for Fix<F> {
        type Output = R;
        fn call(&self, (t,): (T,)) -> R {
            (self.0)(&|t| self.call((t,)), t)
        }
    }
    Fix(f)
}
```

**Just use a regular function**:
```rust
fn factorial(n: i32) -> i32 {
    if n <= 1 { 1 } else { n * factorial(n - 1) }
}
```

### Effect Systems

Some languages track *effects* in types. Closures make this complex:

```
fn map<E>(f: fn() -> () with E, list: List) -> List with E
```

The `with E` says the function might have effect `E`. If you pass a closure that prints, the whole `map` has the printing effect.

Languages with effect systems (Koka, Eff, some research languages) must propagate effects through closures.

---

## Closures and Ownership

In languages with ownership, closures introduce unique challenges.

### The Ownership Question

When a closure captures a variable by move, who owns it now?

```rust
let s = String::from("hello");
let f = move || println!("{}", s);  // s moved into f

// f owns s now
// When f is dropped, s is dropped
```

The closure becomes the owner. Its drop glue must drop captured values.

### Borrowing and Lifetimes

When captured by reference, lifetimes must be tracked:

```rust
fn bad<'a>() -> impl Fn() -> &'a str {
    let s = String::from("hello");
    || s.as_str()  // ERROR: s doesn't live long enough
}
```

The closure captures `&s`, but `s` is dropped when `bad` returns. The lifetime system prevents this at compile time.

### Clone and the Capture Problem

Sometimes you want to clone into a closure:

```rust
let s = String::from("hello");
let f = || println!("{}", s);  // Borrows s
let g = || println!("{}", s);  // Also borrows s
// Fine, multiple immutable borrows allowed

let f = move || println!("{}", s);  // Moves s
let g = move || println!("{}", s);  // ERROR: s already moved
```

Solution: explicit clone:
```rust
let s = String::from("hello");
let s_clone = s.clone();
let f = move || println!("{}", s);
let g = move || println!("{}", s_clone);
```

Rust is getting `clone` in `move` closures to simplify this.

### The Send and Sync Question

Can a closure be sent to another thread?

```rust
let x = Rc::new(42);  // Rc: !Send
let f = || println!("{}", x);

std::thread::spawn(f);  // ERROR: closure is not Send
```

A closure is `Send` only if all its captures are `Send`. This prevents data races at compile time.

Similarly for `Sync`: a closure is `Sync` only if its captures are `Sync`.

### Interior Mutability with Closures

Combining closures with `Cell`, `RefCell`, `Mutex`:

```rust
let count = Cell::new(0);
let increment = || count.set(count.get() + 1);

// increment is Fn, not FnMut!
// Even though it "mutates" count, it does so through shared reference
```

The `Fn` trait sees `&Cell<i32>`, which is `Copy`. The actual mutation happens through interior mutability.

This is important for callbacks in single-threaded contexts:

```rust
let state = RefCell::new(State::new());
button.on_click(|| {
    state.borrow_mut().handle_click();
});
```

---

## Optimization Techniques

Closures can be expensive. Here's how compilers optimize them.

### Inlining

The most important optimization: if a closure is called at a known site, inline it.

Before:
```rust
let f = |x| x + 1;
let y = f(5);
```

After:
```rust
let y = 5 + 1;
```

No closure object, no function call, no captures. This is why Rust's iterator chains are zero-cost—everything inlines.

### Stack Allocation

If a closure doesn't escape, its environment can live on the stack:

```rust
fn process(data: &[i32]) {
    let sum = 0;
    let accumulate = |x| sum += x;  // Closure doesn't escape
    data.iter().for_each(accumulate);
    println!("{}", sum);
}
```

The closure's environment is just a pointer to `sum` on the stack. No heap allocation needed.

### Unboxing

When a closure is passed to a generic function, monomorphization eliminates the abstraction:

```rust
fn apply<F: Fn(i32) -> i32>(f: F, x: i32) -> i32 {
    f(x)
}

let y = apply(|x| x + 1, 5);
```

Compiles to:
```rust
fn apply_closure_0(x: i32) -> i32 {
    x + 1
}

let y = apply_closure_0(5);
```

The closure becomes a zero-sized type (no captures), and the call becomes a direct function call.

### Capture Optimization

Smart capture analysis can reduce closure size:

```rust
let big_array: [i32; 1000] = [...];
let f = || big_array[0];  // Only needs big_array[0], not the whole array!
```

An optimizing compiler might capture just `big_array[0]` (4 bytes) instead of a reference to the array.

### Escape Analysis

Determine if a closure escapes its defining scope:

```rust
fn no_escape() {
    let x = expensive_computation();
    let f = || use(x);
    f();  // Called inline, doesn't escape
}  // x can be on stack

fn escapes() -> Box<dyn Fn()> {
    let x = expensive_computation();
    Box::new(move || use(x))  // Escapes
}  // x must be in the box
```

Non-escaping closures enable stack allocation and better optimization.

### Devirtualization

When a boxed closure is always the same type:

```rust
let f: Box<dyn Fn(i32) -> i32> = Box::new(|x| x + 1);
let y = f(5);  // Virtual call... but we know the type!
```

If analysis shows `f` always holds the same closure type, the virtual call can become a direct call.

---

## Implementation Walkthrough

Let's implement closures for Som step by step.

### Step 1: Syntax

**Closure expressions**:
```
|params| body
|x, y| x + y
|| 42
|x: i32| -> i32 { x + 1 }
```

**Extend the lexer** (`src/lexer/mod.rs`):
```rust
#[token("|")]
Pipe,

#[token("||")]
DoublePipe,  // For empty closures
```

**Extend the AST** (`src/ast/expr.rs`):
```rust
pub enum Expr {
    // ... existing variants ...

    /// Closure expression: |x, y| x + y
    Closure {
        parameters: Vec<ClosureParam>,
        return_type: Option<Type>,
        body: Id<Expr>,
        /// Filled in by upvar analysis
        captures: Vec<Capture>,
    },
}

pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>,
}

pub struct Capture {
    pub name: String,
    pub mode: CaptureMode,
}

pub enum CaptureMode {
    ByValue,
    ByRef,
    ByMutRef,
    ByMove,
}
```

### Step 2: Parsing

**In `src/parser/expr.rs`**:

```rust
fn parse_atom(&mut self) -> Option<Id<Expr>> {
    match self.peek() {
        // Closure: |params| body or || body
        TokenKind::Pipe | TokenKind::DoublePipe => {
            self.parse_closure()
        }
        // ... other cases ...
    }
}

fn parse_closure(&mut self) -> Option<Id<Expr>> {
    let start = self.peek_span();

    // Parse parameters
    let params = if self.eat(TokenKind::DoublePipe) {
        vec![]  // || no parameters
    } else {
        self.expect(TokenKind::Pipe)?;
        let params = self.parse_closure_params()?;
        self.expect(TokenKind::Pipe)?;
        params
    };

    // Optional return type
    let return_type = if self.eat(TokenKind::Arrow) {
        Some(self.parse_type()?)
    } else {
        None
    };

    // Body
    let body = if self.at(TokenKind::OpenBrace) {
        self.parse_block()?
    } else {
        self.parse_expr()?
    };

    let span = start.merge(&self.previous_span());
    Some(self.ast.alloc_expr_with_span(
        Expr::Closure {
            parameters: params,
            return_type,
            body,
            captures: vec![],  // Filled in later
        },
        span,
    ))
}

fn parse_closure_params(&mut self) -> Option<Vec<ClosureParam>> {
    let mut params = vec![];

    while !self.at(TokenKind::Pipe) && !self.at_eof() {
        let (name, _) = self.parse_ident()?;
        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        params.push(ClosureParam { name, ty });

        if !self.at(TokenKind::Pipe) {
            self.expect(TokenKind::Comma)?;
        }
    }

    Some(params)
}
```

### Step 3: Upvar Analysis

**Create `src/upvar_analysis.rs`**:

```rust
use std::collections::{HashMap, HashSet};

pub struct UpvarAnalysis<'ast> {
    ast: &'ast Ast,
    /// Stack of scopes, each containing defined variables
    scopes: Vec<HashSet<String>>,
    /// For each closure, its captured variables
    captures: HashMap<Id<Expr>, Vec<Capture>>,
}

impl<'ast> UpvarAnalysis<'ast> {
    pub fn new(ast: &'ast Ast) -> Self {
        Self {
            ast,
            scopes: vec![HashSet::new()],
            captures: HashMap::new(),
        }
    }

    pub fn analyze(&mut self) {
        // Analyze all functions
        for func in self.ast.funcs.iter() {
            self.enter_scope();
            // Add parameters to scope
            for param in &func.parameters {
                self.define(&param.name.value);
            }
            self.analyze_expr(func.body);
            self.leave_scope();
        }
    }

    fn analyze_expr(&mut self, expr_id: Id<Expr>) {
        let expr = self.ast.exprs.get(&expr_id);

        match expr {
            Expr::Var(name) => {
                // Check if variable is in scope
                if !self.is_defined(&name.value) {
                    // It's a free variable—will be captured
                    self.record_capture(&name.value);
                }
            }

            Expr::Closure { parameters, body, .. } => {
                // Start tracking captures for this closure
                self.enter_closure(expr_id);

                self.enter_scope();
                // Add parameters to inner scope
                for param in parameters {
                    self.define(&param.name.value);
                }
                self.analyze_expr(*body);
                self.leave_scope();

                self.leave_closure();
            }

            Expr::Block { stmts, value } => {
                self.enter_scope();
                for stmt_id in stmts {
                    self.analyze_stmt(*stmt_id);
                }
                if let Some(val) = value {
                    self.analyze_expr(*val);
                }
                self.leave_scope();
            }

            Expr::Binary { lhs, rhs, .. } => {
                self.analyze_expr(*lhs);
                self.analyze_expr(*rhs);
            }

            Expr::Call { args, .. } => {
                for arg in args {
                    self.analyze_expr(*arg);
                }
            }

            // ... handle other expression variants ...

            _ => {}
        }
    }

    fn analyze_stmt(&mut self, stmt_id: Id<Stmt>) {
        let stmt = self.ast.stmts.get(&stmt_id);

        match stmt {
            Stmt::Let { name, value, .. } => {
                self.analyze_expr(*value);
                self.define(&name.value);  // Define AFTER analyzing value
            }
            Stmt::Expr { expr } => {
                self.analyze_expr(*expr);
            }
            // ... other statements ...
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn leave_scope(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn is_defined(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    // ... closure-specific methods ...
}
```

### Step 4: Type Inference

Closures need type inference for parameters and the body:

**In `src/type_check/mod.rs`**:

```rust
fn infer(&mut self, ast: &Ast, expr_id: &Id<Expr>) -> Type {
    match ast.exprs.get(expr_id) {
        Expr::Closure { parameters, return_type, body, captures } => {
            // Enter a new scope for the closure
            self.enter_scope();

            // Create type variables for parameters without annotations
            let param_types: Vec<Type> = parameters.iter().map(|p| {
                let ty = p.ty.clone().unwrap_or_else(|| {
                    Type::Unknown(self.fresh_var())
                });
                self.define(&p.name.value, ty.clone());
                ty
            }).collect();

            // Infer body type
            let body_type = self.infer(ast, body);

            // Check against declared return type if any
            if let Some(declared) = return_type {
                self.unify(&body_type, declared);
            }

            self.leave_scope();

            // The closure type
            Type::Fun {
                arguments: param_types,
                returns: Box::new(body_type),
            }
        }

        // ... other cases ...
    }
}
```

### Step 5: Closure Conversion

Transform closures into structs + functions:

**In `src/closure_convert.rs`**:

```rust
pub struct ClosureConverter<'ast> {
    ast: &'ast mut Ast,
    /// Generated closure structs
    closure_structs: Vec<ClosureStruct>,
    /// Generated closure functions
    closure_funcs: Vec<Func>,
}

pub struct ClosureStruct {
    pub name: String,
    pub captures: Vec<(String, Type)>,
}

impl<'ast> ClosureConverter<'ast> {
    pub fn convert(&mut self, expr_id: Id<Expr>) -> Id<Expr> {
        let expr = self.ast.exprs.get(&expr_id).clone();

        match expr {
            Expr::Closure { parameters, body, captures, .. } => {
                // Generate a unique name
                let closure_name = format!("__closure_{}", self.closure_structs.len());

                // Create the environment struct
                let env_struct = ClosureStruct {
                    name: format!("{}_env", closure_name),
                    captures: captures.iter().map(|c| {
                        (c.name.clone(), self.get_capture_type(c))
                    }).collect(),
                };
                self.closure_structs.push(env_struct);

                // Create the closure function
                let func = self.make_closure_func(
                    &closure_name,
                    &parameters,
                    &captures,
                    body,
                );
                self.closure_funcs.push(func);

                // Replace with: ClosureName { captures... }
                self.make_closure_construction(&closure_name, &captures)
            }

            // Recursively process other expressions
            _ => self.convert_children(expr_id)
        }
    }
}
```

### Step 6: Code Generation

Generate Cranelift code for closure structs and calls:

**In `src/code_gen/mod.rs`**:

```rust
fn gen_closure(&mut self, func: &mut FuncCtx, closure: &ClosureExpr) -> Value {
    let struct_name = &closure.generated_struct_name;
    let struct_def = self.get_closure_struct(struct_name);
    let layout = compute_layout(struct_def);

    // Allocate the closure on the stack
    let slot = func.body.create_sized_stack_slot(StackSlotData::new(
        StackSlotKind::ExplicitSlot,
        layout.size,
        layout.align,
    ));

    let base = func.body.ins().stack_addr(types::I64, slot, 0);

    // Store each capture
    for (i, capture) in closure.captures.iter().enumerate() {
        let value = self.load_variable(func, &capture.name);
        let offset = layout.field_offsets[i] as i32;
        func.body.ins().store(MemFlags::new(), value, base, offset);
    }

    base  // Return pointer to closure
}

fn gen_closure_call(
    &mut self,
    func: &mut FuncCtx,
    closure_val: Value,
    args: &[Value],
) -> Value {
    // Load function pointer from closure (first field)
    let fn_ptr = func.body.ins().load(types::I64, MemFlags::new(), closure_val, 0);

    // Prepare arguments: env pointer first, then actual args
    let mut call_args = vec![closure_val];
    call_args.extend_from_slice(args);

    // Indirect call
    let sig = self.get_closure_signature(/* ... */);
    let sig_ref = func.body.import_signature(sig);
    let call = func.body.ins().call_indirect(sig_ref, fn_ptr, &call_args);

    func.body.inst_results(call)[0]
}
```

### Step 7: The Closure Type

Add a proper closure type:

**In `src/ast/ty.rs`**:

```rust
pub enum Type {
    // ... existing variants ...

    /// Closure type (anonymous, each closure is unique)
    Closure {
        /// Unique ID for this closure
        id: ClosureId,
        /// Captured variables and their types
        captures: Vec<(String, Type)>,
        /// Function signature
        params: Vec<Type>,
        returns: Box<Type>,
    },

    /// Function type (for function pointers, trait objects)
    Fun {
        arguments: Vec<Type>,
        returns: Box<Type>,
    },
}
```

---

## Testing Strategy

### Test 1: Simple Closure (No Captures)

```
fn main() -> i32 {
    let f = |x: i32| x + 1;
    f(5)
}
// Expected: 6
```

This is almost a regular function. Tests basic closure syntax and calling.

### Test 2: Single Capture

```
fn main() -> i32 {
    let a = 10;
    let f = |x: i32| x + a;
    f(5)
}
// Expected: 15
```

Tests capturing a single variable by reference.

### Test 3: Multiple Captures

```
fn main() -> i32 {
    let a = 10;
    let b = 20;
    let f = |x: i32| x + a + b;
    f(5)
}
// Expected: 35
```

Tests capturing multiple variables.

### Test 4: Nested Closures

```
fn main() -> i32 {
    let a = 1;
    let f = || {
        let b = 2;
        let g = || a + b;
        g()
    };
    f()
}
// Expected: 3
```

Tests closures within closures.

### Test 5: Closure as Argument

```
fn apply(f: |i32| -> i32, x: i32) -> i32 {
    f(x)
}

fn main() -> i32 {
    let offset = 10;
    apply(|x| x + offset, 5)
}
// Expected: 15
```

Tests passing closures to functions.

### Test 6: Closure as Return Value

```
fn make_adder(n: i32) -> |i32| -> i32 {
    |x| x + n
}

fn main() -> i32 {
    let add_five = make_adder(5);
    add_five(10)
}
// Expected: 15
```

Tests returning closures (the upward funarg problem).

### Test 7: Mutable Capture

```
fn main() -> i32 {
    let mut count = 0;
    let mut inc = || {
        count = count + 1;
        count
    };
    inc();
    inc();
    inc()
}
// Expected: 3
```

Tests capturing and mutating variables.

### Test 8: Move Semantics

```
fn main() -> i32 {
    let s = make_string();  // Assume returns owned String
    let f = move || string_len(s);
    // s is moved into f
    f()
}
```

Tests move captures.

### Test 9: Closure with FFI

```
extern "libc" {
    fn qsort(base: &mut [i32], compare: |&i32, &i32| -> i32);
}

fn main() -> i32 {
    let mut arr = [3, 1, 4, 1, 5];
    qsort(&mut arr, |a, b| a - b);
    arr[0]  // Should be 1 after sorting
}
```

Tests closures at FFI boundaries.

---

## Further Reading

### Papers

**"Lambda: The Ultimate Declarative"**
*Guy Steele, Gerald Sussman (1976)*
The foundational paper on implementing lambda calculus efficiently.

**"Lambda Lifting"**
*Thomas Johnsson (1985)*
The classic technique for converting closures to top-level functions.

**"Orbit: An Optimizing Compiler for Scheme"**
*David Kranz et al. (1986)*
Detailed treatment of closure conversion in a real compiler.

**"Compiling with Continuations"**
*Andrew Appel (1992)*
Book-length treatment of advanced closure and continuation compilation.

**"Making a Fast Curry"**
*Simon Marlow & Simon Peyton Jones (2006)*
How GHC compiles partial application efficiently.

### Books

**"Essentials of Programming Languages"**
*Friedman, Wand (3rd edition)*
Excellent coverage of closures from a semantic perspective.

**"Lisp in Small Pieces"**
*Christian Queinnec*
Deep dive into implementing Lisp, including closure conversion.

**"Engineering a Compiler"**
*Cooper, Torczon*
Coverage of closure implementation in the optimization chapters.

### Language References

**Rust Reference: Closures**
https://doc.rust-lang.org/reference/expressions/closure-expr.html

**C++ Lambda Expressions**
https://en.cppreference.com/w/cpp/language/lambda

**JavaScript Closures (MDN)**
https://developer.mozilla.org/en-US/docs/Web/JavaScript/Closures

### Blog Posts and Talks

**"How Rust Implements Closures"**
https://huonw.github.io/blog/2015/05/finding-closure-in-rust/

**"Closures: Magic Functions"**
https://craftinginterpreters.com/closures.html
From Crafting Interpreters, excellent walkthrough.

**"The Rust Way of Closures"**
https://smallcultfollowing.com/babysteps/blog/2023/03/29/closures/

---

## Closing Thoughts

Closures are a beautiful example of how programming language theory meets implementation. The funarg problem, discovered in the 1960s, led to:

- **Garbage collection**: To manage closure lifetimes automatically
- **Ownership systems**: To manage lifetimes statically (Rust)
- **Closure conversion**: The transformation that makes closures efficient
- **Escape analysis**: To optimize non-escaping closures

When you implement closures, you're implementing a little piece of history—a solution to a problem that took decades to fully understand.

Start simple:
1. Get the syntax working
2. Implement upvar analysis
3. Do basic closure conversion
4. Add increasingly complex features

The moment your first closure actually captures a variable and works—that's when closures stop being magic and start being machinery you understand.

And once you understand closures, you understand something fundamental about computation: code and data are not so different after all. A closure is code that became data, carrying its context along for the ride.

Good luck, and have fun!

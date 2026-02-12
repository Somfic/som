# Implementing Pattern Matching: Destructuring Data with Elegance

*A comprehensive guide to adding pattern matching to a compiler, exploring the theory, algorithms, and techniques that make this powerful feature work.*

---

## Table of Contents

1. [Introduction: More Than Switch Statements](#introduction-more-than-switch-statements)
2. [A Brief History of Patterns](#a-brief-history-of-patterns)
3. [The Anatomy of Patterns](#the-anatomy-of-patterns)
4. [Pattern Matching Semantics](#pattern-matching-semantics)
5. [Exhaustiveness Checking](#exhaustiveness-checking)
6. [Usefulness and Redundancy](#usefulness-and-redundancy)
7. [Compilation Strategies](#compilation-strategies)
8. [Decision Trees: The Classic Approach](#decision-trees-the-classic-approach)
9. [Optimization Techniques](#optimization-techniques)
10. [Type Checking Patterns](#type-checking-patterns)
11. [Advanced Pattern Features](#advanced-pattern-features)
12. [Implementation Walkthrough](#implementation-walkthrough)
13. [Testing Strategy](#testing-strategy)
14. [Further Reading](#further-reading)

---

## Introduction: More Than Switch Statements

Pattern matching is one of those features that, once you've used it, you can't imagine programming without. It's the ability to simultaneously test the structure of data and extract its components:

```rust
match value {
    Some(x) if x > 0 => println!("Positive: {}", x),
    Some(0) => println!("Zero"),
    Some(x) => println!("Negative: {}", x),
    None => println!("Nothing"),
}
```

This single construct combines:
- **Type discrimination**: Is it `Some` or `None`?
- **Destructuring**: Extract the inner value `x`
- **Binding**: Give it a name we can use
- **Guards**: Additional conditions (`x > 0`)
- **Exhaustiveness**: The compiler ensures we handle all cases

Compare this to the equivalent without pattern matching:

```java
if (value.isPresent()) {
    int x = value.get();
    if (x > 0) {
        System.out.println("Positive: " + x);
    } else if (x == 0) {
        System.out.println("Zero");
    } else {
        System.out.println("Negative: " + x);
    }
} else {
    System.out.println("Nothing");
}
```

The pattern matching version is not just shorter—it's *safer*. The compiler guarantees we handle all cases. It's also more declarative: we describe what we're looking for, not how to look for it.

### Why Pattern Matching Matters

**Safety**: Exhaustiveness checking catches missing cases at compile time. You can't forget to handle `None`.

**Expressiveness**: Complex data transformations become concise and readable.

**Refactoring**: When you add a new variant to an enum, the compiler tells you every place that needs updating.

**Optimization**: Compilers can generate very efficient code from pattern matches—often better than hand-written conditionals.

Understanding pattern matching deeply means understanding:
- **Algebraic data types**: Patterns and ADTs are intimately connected
- **Type theory**: Patterns are a form of type-directed programming
- **Compiler optimization**: Decision tree compilation is a fascinating algorithmic problem
- **Language design**: Pattern syntax and semantics involve many tradeoffs

---

## A Brief History of Patterns

### SNOBOL and String Patterns (1962)

Pattern matching's earliest ancestor is SNOBOL, a language designed for string manipulation:

```snobol
      INPUT 'CAT' : S(SUCCESS) F(FAILURE)
      LINE = "THE CAT SAT"
      LINE 'CAT' = 'DOG'
```

SNOBOL patterns were primarily for strings—find this substring, replace it with that. But the idea of declaratively describing what to match was revolutionary.

### Regular Expressions (1956, formalized 1968)

Stephen Kleene's regular expressions provided a mathematical foundation for string patterns:

```
[a-z]+@[a-z]+\.[a-z]{2,3}
```

Regular expressions became ubiquitous but remained focused on strings.

### ML and Algebraic Pattern Matching (1973)

Robin Milner's ML was the breakthrough. ML combined algebraic data types with pattern matching:

```sml
datatype 'a option = NONE | SOME of 'a

fun unwrap (SOME x) = x
  | unwrap NONE = raise Empty
```

This was different from string patterns. Here, patterns match the *structure* of data, not characters in a string. Patterns could:
- Match constructors (`SOME`, `NONE`)
- Bind variables (`x`)
- Destructure nested data

ML also introduced *exhaustiveness checking*—the compiler warns if patterns don't cover all cases.

### Miranda, Haskell, and Mature Patterns (1985-1990)

Miranda and then Haskell refined pattern matching with:
- Pattern guards
- As-patterns (`x@(y:ys)`)
- Lazy patterns (`~pattern`)
- View patterns (later)

```haskell
length :: [a] -> Int
length [] = 0
length (_:xs) = 1 + length xs

merge :: Ord a => [a] -> [a] -> [a]
merge xs [] = xs
merge [] ys = ys
merge (x:xs) (y:ys)
  | x <= y    = x : merge xs (y:ys)
  | otherwise = y : merge (x:xs) ys
```

### Modern Languages

**Scala (2004)**: Brought pattern matching to the JVM with case classes and extractors.

**Rust (2010)**: Made exhaustive pattern matching a core language feature, integrated with ownership.

**Swift (2014)**: Pattern matching with `switch`, `if case`, `guard case`.

**C# 7+ (2017)**: Added pattern matching incrementally.

**Python 3.10 (2021)**: Finally got structural pattern matching with `match`.

**Java 21 (2023)**: Pattern matching for switch (preview features earlier).

The trend is clear: pattern matching has gone from academic languages to mainstream adoption.

---

## The Anatomy of Patterns

Let's systematically categorize the kinds of patterns languages support.

### Literal Patterns

Match exact values:

```rust
match x {
    0 => "zero",
    1 => "one",
    42 => "the answer",
    _ => "other",
}
```

Literal patterns work for integers, characters, booleans, sometimes strings.

### Variable Patterns

Bind a name to the matched value:

```rust
match opt {
    Some(x) => use(x),  // x is bound
    None => default(),
}
```

The variable `x` becomes available in the arm's body.

### Wildcard Patterns

Match anything, bind nothing:

```rust
match pair {
    (_, 0) => "second is zero",
    (0, _) => "first is zero",
    _ => "neither is zero",
}
```

The underscore `_` says "I don't care about this value."

### Constructor Patterns

Match variants of enums/ADTs:

```rust
enum List<T> {
    Nil,
    Cons(T, Box<List<T>>),
}

match list {
    Nil => "empty",
    Cons(head, tail) => format!("head: {}", head),
}
```

Constructor patterns test which variant and destructure its fields.

### Tuple Patterns

Match product types:

```rust
match point {
    (0, 0) => "origin",
    (x, 0) => format!("on x-axis at {}", x),
    (0, y) => format!("on y-axis at {}", y),
    (x, y) => format!("at ({}, {})", x, y),
}
```

### Struct Patterns

Match named fields:

```rust
match point {
    Point { x: 0, y: 0 } => "origin",
    Point { x, y: 0 } => format!("on x-axis at {}", x),
    Point { x: 0, y } => format!("on y-axis at {}", y),
    Point { x, y } => format!("at ({}, {})", x, y),
}
```

### Nested Patterns

Patterns can nest arbitrarily deep:

```rust
match value {
    Some(Ok((x, Some(y)))) => use(x, y),
    Some(Ok((x, None))) => just_x(x),
    Some(Err(e)) => handle_err(e),
    None => nothing(),
}
```

### Or-Patterns

Match multiple alternatives:

```rust
match digit {
    '0' | '1' => "binary",
    '0'..='7' => "octal",
    '0'..='9' => "decimal",
    'a'..='f' | 'A'..='F' => "hex letter",
    _ => "not a digit",
}
```

### Range Patterns

Match ranges of values:

```rust
match age {
    0..=12 => "child",
    13..=19 => "teenager",
    20..=64 => "adult",
    65.. => "senior",
}
```

### Binding Patterns (As-Patterns)

Bind the whole value while also destructuring:

```rust
match list {
    whole @ Cons(head, _) => {
        println!("list: {:?}, head: {}", whole, head);
    }
    Nil => println!("empty"),
}
```

The `whole @ pattern` syntax binds `whole` to the entire matched value.

### Guard Patterns

Additional boolean conditions:

```rust
match pair {
    (x, y) if x == y => "equal",
    (x, y) if x > y => "first larger",
    (x, y) => "second larger",
}
```

Guards make patterns more expressive but complicate exhaustiveness checking.

### Reference Patterns

Match through references:

```rust
match &option {
    &Some(ref x) => use(x),  // Explicit
    Some(x) => use(x),       // Rust often infers this
    None => {},
}
```

### Box/Deref Patterns

Match heap-allocated data:

```rust
match boxed {
    box 0 => "boxed zero",  // Unstable in Rust
    box x => use(x),
}
```

---

## Pattern Matching Semantics

What exactly does pattern matching *mean*? Let's be precise.

### Matching Relation

A pattern `p` matches a value `v` producing bindings `θ` (a substitution mapping variables to values):

```
p ⊢ v → θ    "pattern p matches value v with bindings θ"
```

The rules:

**Wildcard**: Always matches, binds nothing.
```
_ ⊢ v → {}
```

**Variable**: Always matches, binds the variable.
```
x ⊢ v → {x ↦ v}
```

**Literal**: Matches if equal.
```
n ⊢ v → {}    if n = v
n ⊢ v → ⊥     if n ≠ v
```

**Constructor**: Matches if same constructor and all subpatterns match.
```
C(p₁, ..., pₙ) ⊢ C(v₁, ..., vₙ) → θ₁ ∪ ... ∪ θₙ
    where pᵢ ⊢ vᵢ → θᵢ
```

**Or-pattern**: Matches if any alternative matches.
```
p₁ | p₂ ⊢ v → θ    if p₁ ⊢ v → θ ∨ p₂ ⊢ v → θ
```

### Match Expression Semantics

A match expression evaluates arms in order:

```
match v {
    p₁ => e₁,
    p₂ => e₂,
    ...
}
```

Semantics:
1. Try `p₁ ⊢ v → θ₁`
2. If succeeds, evaluate `e₁[θ₁]` (substitute bindings)
3. If fails, try `p₂`, then `p₃`, etc.
4. If no pattern matches, it's an error (prevented by exhaustiveness checking)

### Guard Semantics

Guards add conditions:

```
match v {
    p if g => e,
    ...
}
```

A guarded pattern matches only if:
1. The pattern matches: `p ⊢ v → θ`
2. The guard evaluates to true: `g[θ] = true`

If the pattern matches but the guard fails, we continue to the next arm.

### Binding vs Testing

Patterns serve two purposes:
1. **Testing**: Does the value have this shape?
2. **Binding**: Extract components and name them.

Some patterns only test (literals, constructors without arguments):
```rust
match x { 0 => ..., 1 => ... }
```

Some patterns only bind (variables, wildcard):
```rust
match x { n => ... }  // n is bound to x
```

Most patterns do both:
```rust
match opt { Some(x) => ... }  // Tests for Some, binds x
```

### Irrefutable Patterns

A pattern is *irrefutable* if it matches all values of its type:

```rust
let (x, y) = point;        // Irrefutable—all pairs match
let Some(x) = opt;         // Refutable—None doesn't match! (error)
```

Irrefutable patterns can be used in `let` bindings, function parameters, and `for` loops. Refutable patterns require `match`, `if let`, or `while let`.

---

## Exhaustiveness Checking

One of pattern matching's killer features is exhaustiveness checking: the compiler verifies that your patterns cover all possible values.

### Why Exhaustiveness Matters

Without exhaustiveness checking:
```rust
match direction {
    North => go_north(),
    South => go_south(),
    // Forgot East and West!
}
```

The program might crash at runtime. With exhaustiveness checking, this is a compile error.

When you add a new variant:
```rust
enum Direction { North, South, East, West, Up, Down }  // Added Up, Down
```

Every `match` on `Direction` that's not exhaustive becomes a compile error. The compiler guides you to every place that needs updating.

### The Exhaustiveness Problem

Given:
- A type `T` with constructors `C₁, C₂, ..., Cₙ`
- A set of patterns `P = {p₁, p₂, ..., pₘ}`

Is `P` exhaustive? Does every value of type `T` match at least one pattern in `P`?

This is decidable for algebraic data types but can be expensive.

### The Matrix Formulation

Luc Maranget's algorithm (2007) is the standard approach. It represents patterns as a matrix:

```
match (x, y) {
    (0, _) => ...
    (_, 0) => ...
    (_, _) => ...
}
```

Pattern matrix:
```
P = | 0  _  |
    | _  0  |
    | _  _  |
```

The algorithm answers: for column of constructors, which rows apply?

### The Algorithm (Simplified)

```python
def is_exhaustive(patterns, type):
    if patterns is empty:
        return False  # No patterns = not exhaustive

    if type is a base type (int, bool, etc.):
        return any pattern is wildcard/variable

    # For ADT with constructors C1, C2, ..., Cn
    constructors = get_constructors(type)

    for C in constructors:
        # Get patterns that match C (or are wildcards)
        matching = [p for p in patterns if matches_constructor(p, C)]

        if not matching:
            return False  # No pattern matches C

        # Check if C's fields are covered
        field_patterns = extract_fields(matching, C)
        if not is_exhaustive(field_patterns, C.field_types):
            return False

    return True
```

### Example: Exhaustiveness Check

```rust
enum Bool { True, False }

match b {
    True => 1,
    False => 0,
}
```

Check:
1. Constructors: `{True, False}`
2. For `True`: patterns `{True}` match. No fields. Covered.
3. For `False`: patterns `{False}` match. No fields. Covered.
4. Result: Exhaustive ✓

```rust
match b {
    True => 1,
}
```

Check:
1. Constructors: `{True, False}`
2. For `True`: patterns `{True}` match. Covered.
3. For `False`: patterns `{}` match. **Not covered!**
4. Result: Not exhaustive ✗

### Witness Generation

When patterns aren't exhaustive, we want to show a *witness*—an example value that isn't matched:

```rust
match opt {
    Some(Some(x)) => ...,
    None => ...,
}
// Warning: non-exhaustive; missing: Some(None)
```

The algorithm can generate witnesses by tracking which constructors are uncovered.

### Guards Complicate Everything

Guards make exhaustiveness checking harder:

```rust
match x {
    n if n > 0 => "positive",
    n if n < 0 => "negative",
    // Is this exhaustive? Is n == 0 covered?
}
```

Most compilers are conservative with guards: they assume a guard might fail, so you need a catch-all or explicit coverage of guard-failing cases.

Some languages (Haskell with GADTs, some dependent types) can prove guards exhaustive in limited cases.

---

## Usefulness and Redundancy

Related to exhaustiveness: are all patterns *useful*?

### The Usefulness Property

A pattern `p` is useful in context `P` if there exists a value that:
1. Matches `p`
2. Doesn't match any pattern in `P`

```rust
match x {
    0 => ...,
    1 => ...,
    0 => ...,  // Useless! Already covered by first arm
    _ => ...,
}
```

The third arm is useless—any value matching `0` already matches the first arm.

### Why Usefulness Matters

Useless patterns indicate bugs:
- **Unreachable code**: Dead code that will never execute
- **Logic errors**: You might have meant a different pattern
- **Refactoring artifacts**: An old pattern no longer makes sense

Good compilers warn about useless patterns.

### The Usefulness Algorithm

Similar to exhaustiveness, but inverted:

```python
def is_useful(pattern, existing_patterns, type):
    # Is there a value matching `pattern` but not `existing_patterns`?

    if existing_patterns is empty:
        return True  # No existing patterns = pattern is useful

    # ... similar recursive decomposition ...
```

### Example: Usefulness Check

```rust
match (x, y) {
    (_, false) => 1,
    (true, _) => 2,
    (false, true) => 3,
    (false, false) => 4,  // Useful?
}
```

For `(false, false)`:
- Row 1 `(_, false)`: matches
- So `(false, false)` is matched by row 1.
- Row 4 is **useless**!

Actually, let me reconsider:
- Row 1 matches `(_, false)` → matches `(false, false)` ✓

So row 4 is indeed useless because row 1 already catches it.

---

## Compilation Strategies

How do we turn pattern matching into efficient code?

### The Naive Approach: Backtracking

Try each arm in order. If pattern fails, backtrack and try the next.

```
match x {
    A(1, _) => e1,
    A(_, 2) => e2,
    B => e3,
}
```

Compiles to:
```
if x is A:
    let (f1, f2) = fields(x)
    if f1 == 1:
        return e1
    if f2 == 2:
        return e2
if x is B:
    return e3
fail()  # Should be unreachable if exhaustive
```

**Problem**: We might test `x is A` twice! This is inefficient.

### Decision Trees

A decision tree tests each component once, branching on the result:

```
            [test x.tag]
           /     |      \
          A      B      ...
         /       |
   [test f1]   return e3
    /    \
   1    else
   |      |
return  [test f2]
  e1    /     \
       2     else
       |       |
    return   fail
      e2
```

Each path through the tree tests each component at most once. This is optimal for number of tests.

### DAG Sharing

Decision trees can have exponential size due to duplicated subtrees. Convert to a DAG:

```
If multiple branches lead to the same subtree, share it.
```

This gives decision DAGs (directed acyclic graphs), which balance code size and speed.

### Backtracking Automata

An alternative: compile to an automaton that can backtrack to try alternatives.

```
State 0: test x.tag
  A -> State 1
  B -> State 4

State 1: test x.f1
  1 -> State 2 (emit e1)
  * -> State 3

State 3: test x.f2
  2 -> emit e2
  * -> fail (or backtrack)

State 4: emit e3
```

This can be more compact than decision trees but involves more runtime bookkeeping.

---

## Decision Trees: The Classic Approach

Let's dive deep into decision tree compilation, the most common approach.

### The Structure

A decision tree is:
```rust
enum DecisionTree {
    // Test a value
    Switch {
        scrutinee: Path,
        branches: Vec<(Constructor, DecisionTree)>,
        default: Option<Box<DecisionTree>>,
    },
    // Successfully matched, execute the arm
    Success {
        arm_index: usize,
        bindings: Vec<(Variable, Path)>,
    },
    // Matching failed (unreachable if patterns exhaustive)
    Failure,
}
```

A `Path` describes how to reach a sub-component: `x.0.1.field`.

### Building Decision Trees

The key insight: we can choose which column to test first. Different choices lead to different trees.

**Left-to-right**: Always test the leftmost column first. Simple, predictable.

**Heuristic-based**: Choose the column that best discriminates patterns. Can produce smaller trees.

### The Algorithm

```python
def compile(patterns_and_bodies, scrutinees, types):
    """
    patterns_and_bodies: [(pattern_row, body_expr), ...]
    scrutinees: [path1, path2, ...]  # What we're matching
    types: [type1, type2, ...]  # Types of scrutinees
    """

    if not patterns_and_bodies:
        return Failure

    # Check if first row matches (all wildcards/variables)
    first_patterns, first_body = patterns_and_bodies[0]
    if all(is_wildcard_or_var(p) for p in first_patterns):
        bindings = extract_bindings(first_patterns, scrutinees)
        return Success(first_body, bindings)

    # Find a column with constructors
    col = select_column(patterns_and_bodies)
    scrutinee = scrutinees[col]
    scrutinee_type = types[col]

    # Get all constructors mentioned in this column
    constructors = collect_constructors(patterns_and_bodies, col)

    # Build branches
    branches = []
    for C in constructors:
        # Specialize: keep rows that match C, expand C's fields
        specialized = specialize(patterns_and_bodies, col, C)
        new_scrutinees = scrutinees[:col] + [field_paths(scrutinee, C)] + scrutinees[col+1:]
        new_types = types[:col] + [field_types(C)] + types[col+1:]

        subtree = compile(specialized, new_scrutinees, new_types)
        branches.append((C, subtree))

    # Default branch for wildcards (if any constructors not mentioned)
    if not covers_all_constructors(constructors, scrutinee_type):
        default_cases = default_rows(patterns_and_bodies, col)
        default_tree = compile(default_cases, scrutinees, types)
    else:
        default_tree = None

    return Switch(scrutinee, branches, default_tree)
```

### Specialization

Specialization is the key operation. When we test constructor `C`:

```
specialize([(A(p1, p2), r), ...], col=0, C=A)
→ [(p1, p2, r), ...]  # Expand A's fields
```

If a row has a wildcard in that column, it matches all constructors:
```
specialize([(_,       r), ...], col=0, C=A)
→ [(_, _,   r), ...]  # Wildcards for A's fields
```

### Example

```rust
match (x, y) {
    (A, A) => 1,
    (A, B) => 2,
    (B, _) => 3,
}
```

Pattern matrix:
```
| A  A |  → 1
| A  B |  → 2
| B  _ |  → 3
```

Step 1: Select column 0 (has constructors A, B)
```
Switch on scrutinee[0]:
  A → ...
  B → ...
```

Step 2a: Specialize for A
```
| A |  → 1
| B |  → 2
```

Recurse: Select column 0
```
Switch on scrutinee[1]:
  A → Success(1)
  B → Success(2)
```

Step 2b: Specialize for B
```
| _ |  → 3
```

First row is wildcard, so:
```
Success(3)
```

Final tree:
```
Switch(x):
  A → Switch(y):
        A → 1
        B → 2
  B → 3
```

### Column Selection Heuristics

Choosing which column to split on affects tree size:

**First non-wildcard**: Simple, left-to-right processing.

**Most constructors**: Split on the column mentioning the most constructors. Maximizes branching.

**Fewest default rows**: Minimize rows that carry over to the default branch.

**Small constructors first**: Test constructors with few fields first, reducing pattern width quickly.

Different heuristics suit different pattern sets. Some compilers try multiple and pick the smallest result.

---

## Optimization Techniques

Decision trees can be optimized further.

### Common Subexpression Elimination

If two branches have identical subtrees, share them:

```
Switch(x):
  A → Switch(y):
        P → body1
        Q → body2
  B → Switch(y):
        P → body1  ← Same as above!
        Q → body2  ← Same as above!
```

After CSE:
```
Switch(x):
  A → shared_subtree
  B → shared_subtree

shared_subtree:
  Switch(y):
    P → body1
    Q → body2
```

### Jump Threading

If a branch goes directly to another branch, shortcut:

```
Switch(x):
  A → goto L1
L1:
  Switch(y):
    ...
```

Can become:
```
Switch(x):
  A → Switch(y): ...  [inlined]
```

### Range Optimization

For numeric patterns, use range checks:

```rust
match n {
    0 => ...,
    1 => ...,
    2 => ...,
    _ => ...,
}
```

Instead of testing `n == 0`, then `n == 1`, then `n == 2`, use a jump table or range check.

### Nested Match Optimization

When patterns have nested structure:

```rust
match x {
    A(B(1)) => ...,
    A(B(2)) => ...,
    A(C) => ...,
    _ => ...,
}
```

The compiler can "fuse" nested matches, testing `x.tag`, then `x.0.tag`, then `x.0.0` in one decision tree.

### Arm Reordering

Sometimes reordering arms produces better code:

```rust
match x {
    rare_case => ...,
    common_case => ...,  // Move up for better branch prediction?
}
```

Most compilers preserve source order for predictability, but some offer hints.

### Constant Propagation Through Matches

If the scrutinee is known:

```rust
let x = Some(5);
match x {
    Some(n) => n + 1,
    None => 0,
}
```

After optimization:
```rust
5 + 1  // The match is eliminated entirely
```

---

## Type Checking Patterns

Patterns must be type-checked to ensure they're valid and to infer bindings.

### The Typing Judgment

```
Γ ⊢ p : T → Γ'
```

"In context Γ, pattern p has type T and extends the context to Γ'."

The extended context Γ' includes bindings introduced by the pattern.

### Typing Rules

**Wildcard**:
```
Γ ⊢ _ : T → Γ
```
Wildcards match any type, introduce no bindings.

**Variable**:
```
Γ ⊢ x : T → Γ, x : T
```
Variables match any type, bind `x` to that type.

**Literal**:
```
Γ ⊢ n : Int → Γ    (if n is an integer literal)
```
Literals have their inherent type.

**Constructor**:
```
C : T₁ × ... × Tₙ → T    (constructor signature)
Γ ⊢ p₁ : T₁ → Γ₁
Γ₁ ⊢ p₂ : T₂ → Γ₂
...
Γₙ₋₁ ⊢ pₙ : Tₙ → Γₙ
────────────────────────────
Γ ⊢ C(p₁, ..., pₙ) : T → Γₙ
```

**Or-pattern**:
```
Γ ⊢ p₁ : T → Γ'
Γ ⊢ p₂ : T → Γ'    (both must produce same bindings!)
───────────────────
Γ ⊢ p₁ | p₂ : T → Γ'
```

### Or-Pattern Binding Consistency

Or-patterns must bind the same variables on both sides:

```rust
match x {
    A(n) | B(n) => use(n),  // OK: both bind n
    A(n) | B(m) => ???,     // Error: different bindings
    A(n) | B => ???,        // Error: left binds n, right doesn't
}
```

This ensures that whatever branch matches, the same variables are available.

### Type Inference with Patterns

Patterns participate in bidirectional type checking:

**Checking mode**: We know the type, check the pattern against it.
```rust
let x: (i32, bool) = ...;
match x {
    (n, true) => ...  // Infer n: i32 from context
}
```

**Inference mode**: We infer the type from the pattern.
```rust
let (a, b) = (1, true);  // Infer (i32, bool)
```

### GADTs and Pattern Matching

With GADTs (Generalized Algebraic Data Types), pattern matching can refine types:

```haskell
data Expr a where
    LitInt  :: Int -> Expr Int
    LitBool :: Bool -> Expr Bool
    Add     :: Expr Int -> Expr Int -> Expr Int
    Eq      :: Expr Int -> Expr Int -> Expr Bool

eval :: Expr a -> a
eval (LitInt n)  = n           -- Here, a ~ Int
eval (LitBool b) = b           -- Here, a ~ Bool
eval (Add e1 e2) = eval e1 + eval e2  -- Here, a ~ Int
eval (Eq e1 e2)  = eval e1 == eval e2 -- Here, a ~ Bool
```

When we match `LitInt`, we learn that `a = Int`. The type system refines the return type accordingly.

This requires sophisticated type checking machinery.

---

## Advanced Pattern Features

### View Patterns

Transform the scrutinee before matching:

```haskell
-- Haskell view patterns
sort :: [Int] -> [Int]
f (sort -> []) = ...
f (sort -> (x:xs)) = ...
```

The expression `sort -> pattern` applies `sort` then matches the result.

### Pattern Synonyms

Named patterns for abstraction:

```haskell
pattern Pair x y = (x, y)
pattern Head x <- (x:_)

f (Pair a b) = a + b  -- Matches tuples
g (Head x) = x        -- Matches non-empty lists
```

Pattern synonyms let you match abstract types without exposing their representation.

### Active Patterns (F#)

User-defined patterns with arbitrary logic:

```fsharp
let (|Even|Odd|) n = if n % 2 = 0 then Even else Odd

match 4 with
| Even -> "even"
| Odd -> "odd"
```

Active patterns blur the line between patterns and functions.

### Pattern Guards Beyond Booleans

Some languages allow pattern guards that themselves match:

```haskell
f x | Just y <- lookup x table = use(y)
    | otherwise = default()
```

The guard `Just y <- lookup x table` both tests and binds.

### First-Class Patterns

Some research languages treat patterns as first-class values:

```
let p = pattern A(x) => x + 1
match value with p  -- Apply pattern
```

This enables pattern composition and abstraction.

### Computed Patterns

Patterns that depend on runtime values:

```rust
match x {
    n if n == computed_value() => ...,  // Guard
    // vs hypothetical computed pattern:
    ${computed_value()} => ...,
}
```

Most languages use guards for this, keeping patterns static.

---

## Implementation Walkthrough

Let's implement pattern matching for Som.

### Step 1: Syntax

**Pattern syntax**:
```
pattern ::= '_'                     // wildcard
          | IDENT                   // variable
          | LITERAL                 // literal
          | IDENT '(' patterns ')'  // constructor
          | '(' patterns ')'        // tuple
          | pattern '|' pattern     // or-pattern
          | IDENT '@' pattern       // as-pattern
          | pattern 'if' expr       // guard

match_expr ::= 'match' expr '{' match_arms '}'
match_arms ::= (pattern '=>' expr ',')*
```

### Step 2: AST

**In `src/ast/pattern.rs`**:

```rust
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard: _
    Wildcard,

    /// Variable binding: x
    Variable(Ident),

    /// Literal: 42, true, "hello"
    Literal(Literal),

    /// Constructor: Some(x), None, Cons(h, t)
    Constructor {
        name: Ident,
        fields: Vec<Id<Pattern>>,
    },

    /// Tuple: (a, b, c)
    Tuple(Vec<Id<Pattern>>),

    /// Or-pattern: A | B
    Or {
        left: Id<Pattern>,
        right: Id<Pattern>,
    },

    /// As-pattern: x @ pattern
    As {
        name: Ident,
        pattern: Id<Pattern>,
    },

    /// Guarded pattern: pattern if condition
    Guard {
        pattern: Id<Pattern>,
        condition: Id<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64),
    Bool(bool),
    String(String),
}
```

**In `src/ast/expr.rs`**:

```rust
pub enum Expr {
    // ... existing variants ...

    /// Match expression
    Match {
        scrutinee: Id<Expr>,
        arms: Vec<MatchArm>,
    },
}

pub struct MatchArm {
    pub pattern: Id<Pattern>,
    pub body: Id<Expr>,
}
```

### Step 3: Parsing Patterns

**In `src/parser/pattern.rs`**:

```rust
impl<'src> Parser<'src> {
    pub fn parse_pattern(&mut self) -> Option<Id<Pattern>> {
        self.parse_or_pattern()
    }

    fn parse_or_pattern(&mut self) -> Option<Id<Pattern>> {
        let mut left = self.parse_primary_pattern()?;

        while self.eat(TokenKind::Pipe) {
            let right = self.parse_primary_pattern()?;
            let span = self.get_span(left).merge(&self.get_span(right));
            left = self.ast.alloc_pattern_with_span(
                Pattern::Or { left, right },
                span,
            );
        }

        Some(left)
    }

    fn parse_primary_pattern(&mut self) -> Option<Id<Pattern>> {
        let start = self.peek_span();

        let pattern = match self.peek() {
            // Wildcard
            TokenKind::Underscore => {
                self.advance();
                Pattern::Wildcard
            }

            // Literal: integer
            TokenKind::Int => {
                let value = self.peek_token().text.parse().unwrap();
                self.advance();
                Pattern::Literal(Literal::Int(value))
            }

            // Literal: boolean
            TokenKind::True => {
                self.advance();
                Pattern::Literal(Literal::Bool(true))
            }
            TokenKind::False => {
                self.advance();
                Pattern::Literal(Literal::Bool(false))
            }

            // Identifier: variable or constructor
            TokenKind::Ident => {
                let name = self.peek_token().text;
                self.advance();

                // Check for constructor pattern: Name(...)
                if self.at(TokenKind::OpenParen) {
                    self.parse_constructor_pattern(name)?
                }
                // Check for as-pattern: name @ pattern
                else if self.eat(TokenKind::At) {
                    let inner = self.parse_primary_pattern()?;
                    Pattern::As {
                        name: self.make_ident(name),
                        pattern: inner,
                    }
                }
                // Plain variable
                else {
                    Pattern::Variable(self.make_ident(name))
                }
            }

            // Tuple pattern: (a, b, c)
            TokenKind::OpenParen => {
                self.advance();
                let mut elements = vec![];
                while !self.at(TokenKind::CloseParen) {
                    elements.push(self.parse_pattern()?);
                    if !self.at(TokenKind::CloseParen) {
                        self.expect(TokenKind::Comma)?;
                    }
                }
                self.expect(TokenKind::CloseParen)?;
                Pattern::Tuple(elements)
            }

            _ => {
                self.error(vec![
                    TokenKind::Underscore,
                    TokenKind::Ident,
                    TokenKind::Int,
                    TokenKind::OpenParen,
                ]);
                return None;
            }
        };

        let span = start.merge(&self.previous_span());
        let pattern_id = self.ast.alloc_pattern_with_span(pattern, span);

        // Check for guard: pattern if condition
        if self.eat(TokenKind::If) {
            let condition = self.parse_expr()?;
            let guard_span = start.merge(&self.previous_span());
            return Some(self.ast.alloc_pattern_with_span(
                Pattern::Guard {
                    pattern: pattern_id,
                    condition,
                },
                guard_span,
            ));
        }

        Some(pattern_id)
    }

    fn parse_constructor_pattern(&mut self, name: &str) -> Option<Pattern> {
        self.expect(TokenKind::OpenParen)?;
        let mut fields = vec![];
        while !self.at(TokenKind::CloseParen) {
            fields.push(self.parse_pattern()?);
            if !self.at(TokenKind::CloseParen) {
                self.expect(TokenKind::Comma)?;
            }
        }
        self.expect(TokenKind::CloseParen)?;

        Some(Pattern::Constructor {
            name: self.make_ident(name),
            fields,
        })
    }
}
```

### Step 4: Parsing Match Expressions

**In `src/parser/expr.rs`**:

```rust
fn parse_atom(&mut self) -> Option<Id<Expr>> {
    match self.peek() {
        TokenKind::Match => self.parse_match_expr(),
        // ... other cases ...
    }
}

fn parse_match_expr(&mut self) -> Option<Id<Expr>> {
    let start = self.peek_span();
    self.expect(TokenKind::Match)?;

    let scrutinee = self.parse_expr()?;
    self.expect(TokenKind::OpenBrace)?;

    let mut arms = vec![];
    while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
        let pattern = self.parse_pattern()?;
        self.expect(TokenKind::FatArrow)?;
        let body = self.parse_expr()?;

        arms.push(MatchArm { pattern, body });

        // Optional comma between arms
        self.eat(TokenKind::Comma);
    }

    self.expect(TokenKind::CloseBrace)?;

    let span = start.merge(&self.previous_span());
    Some(self.ast.alloc_expr_with_span(
        Expr::Match { scrutinee, arms },
        span,
    ))
}
```

### Step 5: Type Checking Patterns

**In `src/type_check/pattern.rs`**:

```rust
impl TypeChecker {
    /// Type check a pattern, returning bindings introduced
    pub fn check_pattern(
        &mut self,
        pattern_id: Id<Pattern>,
        expected_type: &Type,
    ) -> Result<Vec<(String, Type)>, TypeError> {
        let pattern = self.ast.patterns.get(&pattern_id);
        let mut bindings = vec![];

        match pattern {
            Pattern::Wildcard => {
                // Matches anything, no bindings
            }

            Pattern::Variable(name) => {
                bindings.push((name.value.to_string(), expected_type.clone()));
            }

            Pattern::Literal(lit) => {
                let lit_type = self.literal_type(lit);
                self.unify(expected_type, &lit_type)?;
            }

            Pattern::Constructor { name, fields } => {
                // Look up constructor
                let ctor = self.lookup_constructor(&name.value)?;

                // Check we're matching the right type
                self.unify(expected_type, &ctor.result_type)?;

                // Check field count
                if fields.len() != ctor.field_types.len() {
                    return Err(TypeError::WrongFieldCount {
                        expected: ctor.field_types.len(),
                        found: fields.len(),
                    });
                }

                // Check each field pattern
                for (field_pat, field_type) in fields.iter().zip(&ctor.field_types) {
                    let field_bindings = self.check_pattern(*field_pat, field_type)?;
                    bindings.extend(field_bindings);
                }
            }

            Pattern::Tuple(elements) => {
                let Type::Tuple(elem_types) = expected_type else {
                    return Err(TypeError::ExpectedTuple { found: expected_type.clone() });
                };

                if elements.len() != elem_types.len() {
                    return Err(TypeError::WrongTupleSize {
                        expected: elem_types.len(),
                        found: elements.len(),
                    });
                }

                for (elem_pat, elem_type) in elements.iter().zip(elem_types) {
                    let elem_bindings = self.check_pattern(*elem_pat, elem_type)?;
                    bindings.extend(elem_bindings);
                }
            }

            Pattern::Or { left, right } => {
                let left_bindings = self.check_pattern(*left, expected_type)?;
                let right_bindings = self.check_pattern(*right, expected_type)?;

                // Both sides must bind the same variables with same types
                self.check_binding_consistency(&left_bindings, &right_bindings)?;
                bindings = left_bindings;
            }

            Pattern::As { name, pattern } => {
                bindings.push((name.value.to_string(), expected_type.clone()));
                let inner_bindings = self.check_pattern(*pattern, expected_type)?;
                bindings.extend(inner_bindings);
            }

            Pattern::Guard { pattern, condition } => {
                let pattern_bindings = self.check_pattern(*pattern, expected_type)?;

                // Check guard with pattern bindings in scope
                self.with_bindings(&pattern_bindings, |tc| {
                    let cond_type = tc.infer_expr(*condition)?;
                    tc.unify(&Type::Bool, &cond_type)?;
                    Ok(())
                })?;

                bindings = pattern_bindings;
            }
        }

        Ok(bindings)
    }
}
```

### Step 6: Exhaustiveness Checking

**In `src/exhaustiveness.rs`**:

```rust
pub struct ExhaustivenessChecker<'ast> {
    ast: &'ast Ast,
}

impl<'ast> ExhaustivenessChecker<'ast> {
    pub fn check_match(
        &self,
        scrutinee_type: &Type,
        arms: &[MatchArm],
    ) -> Result<(), Vec<Pattern>> {
        let patterns: Vec<_> = arms.iter().map(|a| a.pattern).collect();

        if self.is_exhaustive(&patterns, scrutinee_type) {
            Ok(())
        } else {
            let witnesses = self.find_witnesses(&patterns, scrutinee_type);
            Err(witnesses)
        }
    }

    fn is_exhaustive(&self, patterns: &[Id<Pattern>], ty: &Type) -> bool {
        if patterns.is_empty() {
            return false;
        }

        // Check if first pattern is irrefutable
        if self.is_irrefutable(patterns[0], ty) {
            return true;
        }

        // Get all constructors of this type
        let constructors = self.get_constructors(ty);

        for ctor in &constructors {
            // Specialize patterns for this constructor
            let specialized = self.specialize(patterns, ctor);

            if specialized.is_empty() {
                return false; // No pattern matches this constructor
            }

            // Recursively check fields
            let field_types = self.get_field_types(ctor);
            if !self.is_exhaustive_tuple(&specialized, &field_types) {
                return false;
            }
        }

        true
    }

    fn specialize(
        &self,
        patterns: &[Id<Pattern>],
        ctor: &Constructor,
    ) -> Vec<Vec<Id<Pattern>>> {
        patterns.iter().filter_map(|&p| {
            self.specialize_pattern(p, ctor)
        }).collect()
    }

    fn specialize_pattern(
        &self,
        pattern: Id<Pattern>,
        ctor: &Constructor,
    ) -> Option<Vec<Id<Pattern>>> {
        let pat = self.ast.patterns.get(&pattern);

        match pat {
            Pattern::Wildcard | Pattern::Variable(_) => {
                // Wildcards match any constructor
                Some(vec![self.wildcard(); ctor.arity])
            }

            Pattern::Constructor { name, fields } => {
                if name.value == ctor.name {
                    Some(fields.clone())
                } else {
                    None // Different constructor, doesn't match
                }
            }

            Pattern::Or { left, right } => {
                // Try both branches
                self.specialize_pattern(*left, ctor)
                    .or_else(|| self.specialize_pattern(*right, ctor))
            }

            Pattern::As { pattern, .. } => {
                self.specialize_pattern(*pattern, ctor)
            }

            Pattern::Guard { pattern, .. } => {
                // Conservatively treat guards as potentially failing
                // So specialize the pattern but mark as uncertain
                self.specialize_pattern(*pattern, ctor)
            }

            _ => None,
        }
    }
}
```

### Step 7: Decision Tree Compilation

**In `src/pattern_compile.rs`**:

```rust
pub enum DecisionTree {
    /// Test scrutinee and branch
    Switch {
        path: ScrutineePath,
        branches: Vec<(Constructor, DecisionTree)>,
        default: Option<Box<DecisionTree>>,
    },

    /// Successfully matched arm
    Success {
        arm_index: usize,
        bindings: Vec<(String, ScrutineePath)>,
    },

    /// Match failed (should be unreachable if exhaustive)
    Failure,
}

pub struct PatternCompiler<'ast> {
    ast: &'ast Ast,
}

impl<'ast> PatternCompiler<'ast> {
    pub fn compile(
        &self,
        arms: &[MatchArm],
        scrutinee: ScrutineePath,
        scrutinee_type: &Type,
    ) -> DecisionTree {
        self.compile_matrix(
            &self.build_matrix(arms),
            &[scrutinee],
            &[scrutinee_type.clone()],
        )
    }

    fn compile_matrix(
        &self,
        matrix: &PatternMatrix,
        scrutinees: &[ScrutineePath],
        types: &[Type],
    ) -> DecisionTree {
        // Empty matrix = failure
        if matrix.is_empty() {
            return DecisionTree::Failure;
        }

        // Check if first row matches unconditionally
        if self.first_row_matches(matrix) {
            return DecisionTree::Success {
                arm_index: matrix.rows[0].arm_index,
                bindings: self.extract_bindings(&matrix.rows[0], scrutinees),
            };
        }

        // Select column to split on
        let col = self.select_column(matrix);
        let scrutinee = &scrutinees[col];
        let scrutinee_type = &types[col];

        // Get constructors in this column
        let constructors = self.collect_constructors(matrix, col, scrutinee_type);

        // Build branches
        let mut branches = vec![];
        for ctor in &constructors {
            let specialized = self.specialize_matrix(matrix, col, ctor);
            let new_scrutinees = self.expand_scrutinees(scrutinees, col, scrutinee, ctor);
            let new_types = self.expand_types(types, col, ctor);

            let subtree = self.compile_matrix(&specialized, &new_scrutinees, &new_types);
            branches.push((ctor.clone(), subtree));
        }

        // Build default branch if needed
        let default = if !self.covers_all(scrutinee_type, &constructors) {
            let defaulted = self.default_matrix(matrix, col);
            Some(Box::new(self.compile_matrix(&defaulted, scrutinees, types)))
        } else {
            None
        };

        DecisionTree::Switch {
            path: scrutinee.clone(),
            branches,
            default,
        }
    }
}
```

### Step 8: Code Generation

**In `src/code_gen/pattern.rs`**:

```rust
impl<'ast> Codegen<'ast> {
    pub fn gen_match(
        &mut self,
        func: &mut FuncCtx,
        scrutinee: Id<Expr>,
        arms: &[MatchArm],
    ) -> Value {
        // Evaluate scrutinee
        let scrutinee_val = self.gen_expr(func, scrutinee);
        let scrutinee_type = self.get_type(&scrutinee);

        // Compile to decision tree
        let tree = self.pattern_compiler.compile(
            arms,
            ScrutineePath::Root(scrutinee_val),
            &scrutinee_type,
        );

        // Generate code from decision tree
        self.gen_decision_tree(func, &tree)
    }

    fn gen_decision_tree(
        &mut self,
        func: &mut FuncCtx,
        tree: &DecisionTree,
    ) -> Value {
        match tree {
            DecisionTree::Success { arm_index, bindings } => {
                // Bind variables
                for (name, path) in bindings {
                    let value = self.load_path(func, path);
                    let var = func.body.declare_var(self.get_path_type(path));
                    func.body.def_var(var, value);
                    func.env.insert(name.clone(), var);
                }

                // Generate arm body
                let arm = &self.current_arms[*arm_index];
                self.gen_expr(func, arm.body)
            }

            DecisionTree::Failure => {
                // Should be unreachable if exhaustiveness checking passed
                func.body.ins().trap(TrapCode::UnreachableCodeReached)
            }

            DecisionTree::Switch { path, branches, default } => {
                // Load discriminant
                let discriminant = self.load_discriminant(func, path);

                // Create blocks for each branch
                let merge_block = func.body.create_block();
                let result_type = self.get_result_type();
                let result_param = func.body.append_block_param(merge_block, result_type);

                for (ctor, subtree) in branches {
                    let branch_block = func.body.create_block();
                    let next_block = func.body.create_block();

                    // Test discriminant
                    let matches = self.test_constructor(func, discriminant, ctor);
                    func.body.ins().brif(matches, branch_block, &[], next_block, &[]);

                    // Generate branch body
                    func.body.switch_to_block(branch_block);
                    func.body.seal_block(branch_block);
                    let result = self.gen_decision_tree(func, subtree);
                    func.body.ins().jump(merge_block, &[result]);

                    func.body.switch_to_block(next_block);
                    func.body.seal_block(next_block);
                }

                // Generate default if present
                if let Some(default_tree) = default {
                    let result = self.gen_decision_tree(func, default_tree);
                    func.body.ins().jump(merge_block, &[result]);
                } else {
                    func.body.ins().trap(TrapCode::UnreachableCodeReached);
                }

                func.body.switch_to_block(merge_block);
                func.body.seal_block(merge_block);
                result_param
            }
        }
    }
}
```

---

## Testing Strategy

### Test 1: Simple Literal Match

```
fn main() -> i32 {
    let x = 2;
    match x {
        1 => 10,
        2 => 20,
        _ => 30,
    }
}
// Expected: 20
```

### Test 2: Constructor Match

```
enum Option { Some(i32), None }

fn main() -> i32 {
    let opt = Some(42);
    match opt {
        Some(x) => x,
        None => 0,
    }
}
// Expected: 42
```

### Test 3: Nested Patterns

```
fn main() -> i32 {
    let nested = Some(Some(5));
    match nested {
        Some(Some(x)) => x,
        Some(None) => -1,
        None => -2,
    }
}
// Expected: 5
```

### Test 4: Tuple Patterns

```
fn main() -> i32 {
    let pair = (1, 2);
    match pair {
        (0, y) => y,
        (x, 0) => x,
        (x, y) => x + y,
    }
}
// Expected: 3
```

### Test 5: Or-Patterns

```
fn main() -> i32 {
    let x = 3;
    match x {
        1 | 2 => 10,
        3 | 4 => 20,
        _ => 30,
    }
}
// Expected: 20
```

### Test 6: Guards

```
fn main() -> i32 {
    let x = 5;
    match x {
        n if n < 0 => -1,
        n if n == 0 => 0,
        n => 1,
    }
}
// Expected: 1
```

### Test 7: As-Patterns

```
enum List { Cons(i32, Box<List>), Nil }

fn main() -> i32 {
    let list = Cons(1, Box::new(Cons(2, Box::new(Nil))));
    match list {
        whole @ Cons(head, _) => head,
        Nil => 0,
    }
}
// Expected: 1
```

### Test 8: Exhaustiveness Error

```
enum Color { Red, Green, Blue }

fn main() -> i32 {
    let c = Red;
    match c {
        Red => 1,
        Green => 2,
        // Missing Blue!
    }
}
// Expected: Compile error, non-exhaustive
```

### Test 9: Useless Pattern Warning

```
fn main() -> i32 {
    let x = 5;
    match x {
        _ => 1,
        5 => 2,  // Useless!
    }
}
// Expected: Warning, unreachable pattern
```

### Test 10: Complex Nested Match

```
enum Expr {
    Lit(i32),
    Add(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
}

fn eval(e: Expr) -> i32 {
    match e {
        Lit(n) => n,
        Add(l, r) => eval(*l) + eval(*r),
        Mul(l, r) => eval(*l) * eval(*r),
    }
}

fn main() -> i32 {
    // 2 + 3 * 4 = 14
    let expr = Add(
        Box::new(Lit(2)),
        Box::new(Mul(Box::new(Lit(3)), Box::new(Lit(4))))
    );
    eval(expr)
}
// Expected: 14
```

---

## Further Reading

### Papers

**"Compiling Pattern Matching to Good Decision Trees"**
*Luc Maranget (2008)*
The definitive paper on pattern compilation. Introduces the matrix-based algorithm.

**"Warnings for Pattern Matching"**
*Luc Maranget (2007)*
Exhaustiveness and usefulness checking algorithms.

**"When Do Match-Compilation Heuristics Matter?"**
*Kevin Scott & Norman Ramsey (2000)*
Empirical study of column selection heuristics.

**"ML Pattern Match Compilation and Partial Evaluation"**
*Peter Sestoft (1996)*
Earlier foundational work on pattern compilation.

**"GADTs Meet Their Match"**
*George Karachalias et al. (2015)*
Pattern matching with GADTs and type refinement.

### Books

**"The Implementation of Functional Programming Languages"**
*Simon Peyton Jones (1987)*
Chapter 5 covers pattern matching compilation in depth.

**"Modern Compiler Implementation in ML"**
*Andrew Appel (1998)*
Good coverage of pattern matching in the context of ML.

**"Compiling with Continuations"**
*Andrew Appel (1992)*
Patterns in the CPS transformation.

### Language Documentation

**Rust Reference: Patterns**
https://doc.rust-lang.org/reference/patterns.html

**Haskell Pattern Matching**
https://www.haskell.org/onlinereport/haskell2010/haskellch3.html

**OCaml Manual: Patterns**
https://v2.ocaml.org/manual/patterns.html

**Scala Pattern Matching**
https://docs.scala-lang.org/tour/pattern-matching.html

### Implementations to Study

**rustc Pattern Matching**
https://github.com/rust-lang/rust/tree/master/compiler/rustc_mir_build/src/thir/pattern

**GHC Pattern Match Checker**
https://gitlab.haskell.org/ghc/ghc/-/tree/master/compiler/GHC/HsToCore/Pmc

**OCaml Pattern Compilation**
https://github.com/ocaml/ocaml/tree/trunk/lambda

### Blog Posts

**"How OCaml Compiles Pattern Matching"**
https://www.lri.fr/~filliatr/ftp/publis/patterns.pdf

**"Rust's Pattern Matching: How It Works"**
Various blog posts on the Rust internals.

---

## Closing Thoughts

Pattern matching is one of those features where the surface simplicity hides deep complexity. A simple `match` expression involves:

- **Parsing**: A whole sub-grammar for patterns
- **Type checking**: Bidirectional flow, binding introduction
- **Exhaustiveness**: A sophisticated coverage algorithm
- **Usefulness**: Detecting dead code
- **Compilation**: Decision trees with optimization

But when you get it right, pattern matching transforms how people write code. It makes destructuring natural, makes exhaustive handling automatic, and makes data-driven code beautiful.

The Maranget algorithm, decision trees, and exhaustiveness checking are gems of compiler engineering. They're simple enough to understand in an afternoon, elegant enough to appreciate for years.

Start with the basics:
1. Parse simple patterns (wildcards, variables, constructors)
2. Type check patterns against scrutinee types
3. Compile to nested if-else (naive but correct)
4. Add exhaustiveness checking
5. Optimize with decision trees
6. Add advanced features (guards, or-patterns, etc.)

When your first complex pattern match compiles to an efficient decision tree, and the compiler catches a missing case before runtime does—that's when you understand why functional programmers love pattern matching so much.

Good luck, and enjoy the patterns!

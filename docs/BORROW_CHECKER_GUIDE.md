# Borrow Checker Implementation Guide

This guide walks you through implementing a borrow checker for Som, explaining the concepts and reasoning at each step.

---

## Table of Contents

1. [What is a Borrow Checker?](#1-what-is-a-borrow-checker)
2. [The Core Problem](#2-the-core-problem)
3. [Key Concepts](#3-key-concepts)
4. [Data Structures](#4-data-structures)
5. [The Algorithm](#5-the-algorithm)
6. [Error Reporting](#6-error-reporting)
7. [Integration](#7-integration)

---

## 1. What is a Borrow Checker?

A borrow checker is a static analysis pass that ensures **memory safety** without garbage collection. It enforces rules at compile time that prevent:

- **Use-after-free**: Using memory after it's been deallocated
- **Double-free**: Deallocating the same memory twice
- **Dangling pointers**: References that point to invalid memory
- **Data races**: Multiple threads accessing memory unsafely

The key insight is that these bugs stem from **aliasing + mutation**. If you have multiple references to the same data and one of them mutates it, bad things happen. The borrow checker prevents this by controlling when and how references can exist.

---

## 2. The Core Problem

Consider this code:

```
let x = 42
let r = &x      // r borrows x
let y = x       // x is moved to y
print(*r)       // ERROR: r points to x, but x was moved!
```

The variable `r` holds a reference to `x`. When we move `x` to `y`, the memory location `r` points to is no longer valid. This is a **use-after-move** bug.

Another problematic case:

```
let x = 42
let r1 = &x      // immutable borrow
let r2 = &mut x  // mutable borrow - ERROR!
```

If we allow both `&x` and `&mut x` to exist simultaneously, code using `r1` might see unexpected changes made through `r2`. This violates the principle that immutable references should see stable data.

**The Rules:**
1. A value can have **one mutable reference** OR **any number of immutable references**, but not both
2. References must not outlive the data they point to
3. Once a value is moved, it cannot be used

---

## 3. Key Concepts

### 3.1 Places

A **place** is a memory location that can hold a value. In our simple implementation, places are just local variables. Each place has:

- A **name** (the variable name)
- A **scope depth** (which block it was created in)

The scope depth is crucial for detecting dangling references. If a variable is created in scope 2 and we try to return a reference to it from scope 1, that's an error.

### 3.2 Value States

Each place can be in one of these states:

| State | Meaning |
|-------|---------|
| **Owned** | The value is here and available for use |
| **Moved** | The value has been moved elsewhere; using it is an error |
| **Borrowed** | One or more immutable references exist |
| **MutBorrowed** | A mutable reference exists |

State transitions:
```
Owned -> Moved       (when assigned to another variable)
Owned -> Borrowed    (when &x is taken)
Owned -> MutBorrowed (when &mut x is taken)
Borrowed -> Owned    (when all borrows expire)
MutBorrowed -> Owned (when the mutable borrow expires)
```

### 3.3 Loans

A **loan** represents an active borrow. When you write `&x`, you create a loan. Loans have:

- The **place** being borrowed (which variable)
- Whether it's **mutable** or not
- The **scope depth** where it was created

Loans **expire** when their scope ends. This is lexical lifetime - the borrow lives until the end of the block where it was created.

### 3.4 Reference Origins

To detect dangling references, we track where each reference "comes from":

- **Local**: Points to a local variable (has a scope depth)
- **Parameter**: Points to a function parameter (lives for the whole function)
- **Static**: Points to static data (lives forever)

When returning a reference, we check: does the reference origin outlive the return target? If a reference points to a local variable that will be dropped, that's a dangling reference error.

### 3.5 Copy Types

Some types are **Copy** - using them doesn't move the value, it copies it. In Som, these are:

- `i32`, `bool`, `()` (primitives)
- References (`&T`, `&mut T`) - the reference itself is copied, not the data

For Copy types, we don't track moves - you can use them as many times as you want.

---

## 4. Data Structures

### 4.1 PlaceId and Place

You need a way to identify places. Use a simple newtype index:

```
PlaceId(u32)  // Index into a Vec<Place>
```

Each `Place` stores the variable name and the scope depth where it was created.

### 4.2 LoanId and Loan

Similarly for loans:

```
LoanId(u32)  // Index into a Vec<Loan>
```

Each `Loan` stores:
- Which place is borrowed
- Whether it's mutable
- The expression that created the borrow (for error messages)
- The scope depth (to know when it expires)

### 4.3 ValueState

An enum tracking the current state of each place:
- `Owned` - available
- `Moved { at: ExprId }` - moved away at this expression
- `Borrowed { loans: Vec<LoanId> }` - has active immutable borrows
- `MutBorrowed { loan: LoanId }` - has an active mutable borrow

### 4.4 RefOrigin

Tracks where a reference points to:
- `Local { place, scope_depth }` - points to a local variable
- `Parameter` - points to a function parameter
- `Static` - points to static data

### 4.5 BorrowChecker

The main struct holding all the state:
- `places: Vec<Place>` - all known places
- `name_to_place: HashMap<String, PlaceId>` - lookup by name
- `place_states: HashMap<PlaceId, ValueState>` - current state of each place
- `loans: Vec<Loan>` - all loans ever created
- `ref_origins: HashMap<ExprId, RefOrigin>` - origin of each borrow expression
- `scope_depth: u32` - current nesting level
- `errors: Vec<BorrowError>` - collected errors

---

## 5. The Algorithm

The borrow checker walks the typed AST in execution order, simulating what happens to values at runtime.

### 5.1 Entry Point

For each function:
1. Create the `BorrowChecker`
2. Register function parameters as places (scope depth 0, state Owned)
3. Mark parameter places with `RefOrigin::Parameter` if they're references
4. Check the function body
5. Collect errors

### 5.2 Checking Expressions

For each expression type:

**Literals (`42`, `true`):**
- No borrow checking needed

**Variables (`x`):**
- Call `check_use(name)` to verify the variable is usable
- If the type is not Copy and this is a "consuming" context, call `check_move()`

**Blocks (`{ ... }`):**
1. Increment `scope_depth`
2. Check all statements
3. Check the return value (if any)
4. If there's a return value with reference type, call `check_return()`
5. Decrement `scope_depth` and expire loans from this scope

**Binary operations (`a + b`):**
- Check both operands (they're reads, not moves, for primitives)

**Function calls (`f(a, b)`):**
- For each argument, call `check_move()` (arguments are moved into the function)
- Exception: if the parameter type is a reference, the argument is borrowed, not moved

**Borrows (`&x`, `&mut x`):**
- Call `check_borrow(mutable, inner_expr)`
- Record the `RefOrigin` for this expression

**Dereferences (`*r`):**
- Check the inner expression
- Dereferencing reads through the reference

### 5.3 check_use(name)

Verifies a variable can be read:

1. Look up the `PlaceId` for this name
2. Get its current `ValueState`
3. Match on the state:
   - `Owned` or `Borrowed` → OK
   - `Moved { at }` → Error: use after move
   - `MutBorrowed { loan }` → Error: use while mutably borrowed

### 5.4 check_move(expr)

Handles moving a value:

1. If the expression's type is Copy, just call `check_use()` instead
2. If it's a variable:
   - Call `check_use()` first
   - Check state isn't `Borrowed` or `MutBorrowed` (can't move borrowed value)
   - Update state to `Moved { at: expr_id }`
3. Otherwise, recursively check sub-expressions

### 5.5 check_borrow(mutable, inner_expr, borrow_expr)

Creates a new borrow:

1. The inner expression must be a variable (for now)
2. Look up the place
3. Check current state for conflicts:

| Current State | New `&` | New `&mut` |
|--------------|---------|------------|
| Owned | OK, add loan | OK, add loan |
| Moved | ERROR: use after move | ERROR: use after move |
| Borrowed | OK, add loan | ERROR: conflicting borrow |
| MutBorrowed | ERROR: conflicting borrow | ERROR: conflicting borrow |

4. Create a `Loan` and update the place's state
5. Record the `RefOrigin` for this borrow expression

### 5.6 check_return(expr, target_scope)

Checks if returning a reference is safe:

1. Get the expression's type from the typed AST
2. If it's not a reference type, nothing to check
3. Look up the `RefOrigin` for this expression
4. If the origin is `Local { scope_depth }`:
   - If `scope_depth >= target_scope`, ERROR: dangling reference
   - The reference would outlive the local variable
5. `Parameter` and `Static` origins are always safe to return

### 5.7 Scope Management

**push_scope():**
- Increment `scope_depth`

**pop_scope():**
1. Find all loans where `loan.scope_depth == current_depth`
2. For each expiring loan:
   - Remove it from the place's state
   - If a place goes from `Borrowed` with one loan to empty, restore to `Owned`
   - If a place goes from `MutBorrowed` to no loan, restore to `Owned`
3. Remove local variables created in this scope from `name_to_place`
4. Decrement `scope_depth`

---

## 6. Error Reporting

Each error should produce a helpful diagnostic. Use the existing `Diagnostic` infrastructure with primary and secondary labels.

### Error Types

**UseAfterMove:**
```
error: use of moved value: `x`
   │
 3 │ let y = x;
   │         ━ value moved here
 4 │ let z = x;
   │         ━ value used here after move
   │
= hint: consider using `&x` to borrow instead of moving
```

**ConflictingBorrow:**
```
error: cannot borrow `x` as mutable because it is already borrowed as immutable
   │
 2 │ let r1 = &x;
   │          ── immutable borrow occurs here
 3 │ let r2 = &mut x;
   │          ━━━━━━ mutable borrow occurs here
```

**DanglingReference:**
```
error: cannot return reference to local variable `x`
   │
 2 │ let x = 42;
   │     ─ `x` is created here
 3 │ &x
   │ ━━ returns a reference to `x`
   │
= hint: `x` will be dropped when the function returns
```

---

## 7. Integration

### 7.1 Pipeline Position

```
Source → Lexer → Parser → Type Checker → Borrow Checker → Code Gen
                              │                │
                          TypedAst ────────────┘
```

The borrow checker runs after type checking because:
- It needs to know which types are Copy
- It needs to know which expressions have reference types
- Type errors should be reported before borrow errors

### 7.2 Calling the Borrow Checker

In `main.rs`, after type checking:

```
// Type check
let typed_ast = inferencer.check_program(ast);

// Report type errors first
for error in &typed_ast.errors { ... }

// Borrow check
let borrow_errors = borrow_check::check_program(&typed_ast);

// Report borrow errors
for error in &borrow_errors { ... }
```

### 7.3 Module Structure

```
src/
├── borrow_check/
│   ├── mod.rs     # BorrowChecker implementation
│   └── error.rs   # BorrowError enum and diagnostics
└── main.rs        # Integration point
```

---

## Next Steps

After implementing the basic borrow checker:

1. **Test with examples** - Try the test cases to verify correctness
2. **Improve error messages** - Add hints and suggestions
3. **Handle more cases** - Field access, indexing, etc.
4. **Consider NLL** - Non-lexical lifetimes for fewer false positives

Good luck with your implementation!

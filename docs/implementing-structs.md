# Implementing Structs: A Deep Dive

*A comprehensive guide to adding struct support to a compiler, exploring the theory, history, and fascinating ideas that make aggregate types work.*

---

## Table of Contents

1. [Introduction: More Than Just Data](#introduction-more-than-just-data)
2. [A Brief History of Records](#a-brief-history-of-records)
3. [Type Theory: Products, Records, and Beyond](#type-theory-products-records-and-beyond)
4. [Memory: Where Bits Meet Silicon](#memory-where-bits-meet-silicon)
5. [Parsing: Teaching Your Compiler New Syntax](#parsing-teaching-your-compiler-new-syntax)
6. [Type Checking: Ensuring Correctness](#type-checking-ensuring-correctness)
7. [Code Generation: From Abstract to Concrete](#code-generation-from-abstract-to-concrete)
8. [The ABI: Where Compilers Must Agree](#the-abi-where-compilers-must-agree)
9. [Advanced Topics: Where Things Get Interesting](#advanced-topics-where-things-get-interesting)
10. [Implementation Walkthrough](#implementation-walkthrough)
11. [Testing Strategy](#testing-strategy)
12. [Further Reading](#further-reading)

---

## Introduction: More Than Just Data

At first glance, structs seem almost trivially simple. They're just a way to group related data together, right? A point has an x and a y. A person has a name and an age. What's so complicated about that?

But structs are one of those features that, like an iceberg, hide enormous complexity beneath a simple surface. Implementing structs properly requires understanding:

- **Type theory**: What does it mean for two types to be "the same"?
- **Memory architecture**: Why does the order of fields matter?
- **CPU design**: Why can't you just put bytes wherever you want?
- **Operating systems**: How do programs talk to each other?
- **Calling conventions**: Who decides where arguments go?
- **Compiler optimization**: Can a struct exist only in registers?

By the time you've implemented structs with proper FFI support, you'll have touched nearly every part of your compiler and learned fundamental concepts that apply far beyond this single feature.

Let's begin.

---

## A Brief History of Records

### The Mathematical Origins

The concept of grouping related data predates computers entirely. In mathematics, an *ordered pair* (a, b) combines two values into one. The Cartesian product A × B represents all possible pairs of elements from sets A and B.

René Descartes gave us Cartesian coordinates in the 17th century—arguably the first "struct" with fields x and y. Every point on a plane could be described by this pair of numbers.

### COBOL and the Business Record (1959)

COBOL introduced one of the first programming language implementations of structured data. In a world of punch cards and fixed-width fields, COBOL's record structures mapped directly to physical data layouts:

```cobol
01 EMPLOYEE-RECORD.
   05 EMPLOYEE-ID     PIC 9(5).
   05 EMPLOYEE-NAME   PIC X(30).
   05 SALARY          PIC 9(7)V99.
```

This wasn't abstract—it described exactly how data was laid out on a card or tape. The numbers (01, 05) indicated nesting levels. COBOL programmers thought in terms of physical storage.

### ALGOL and Pascal: The Academic Tradition

While COBOL served business, academics were developing more principled approaches. ALGOL W (1966) introduced the `record` type:

```algol
record PERSON (string NAME; integer AGE);
```

Pascal (1970) refined this into a form we'd recognize today:

```pascal
type
  Point = record
    x: real;
    y: real;
  end;
```

Pascal's records were designed for clarity and safety, not machine efficiency. Niklaus Wirth prioritized teaching and correctness over performance.

### C: The Machine's View (1972)

C changed everything. Dennis Ritchie designed C to write Unix, and C's structs reflect this systems programming heritage:

```c
struct point {
    float x;
    float y;
};
```

Unlike Pascal, C's structs have a guaranteed memory layout. You can take the address of any field. You can cast struct pointers to char pointers and examine the bytes. This low-level access was essential for writing operating systems but introduced complexity we're still dealing with today.

C's struct layout rules—alignment, padding, the works—became the de facto standard. When we talk about "C ABI compatibility," we're really talking about matching how C compilers lay out structs.

### Modern Languages

Different languages made different choices:

**C++ (1983)**: Extended C structs with methods, access control, and inheritance. A C++ `struct` is really just a `class` with public default access.

**Java (1995)**: Rejected structs entirely (initially). Everything is an object, everything is on the heap, everything is a reference. This simplified the language but introduced performance implications that Java is still addressing with Project Valhalla's "value types."

**Go (2009)**: Back to basics with simple structs, but no inheritance. Composition over inheritance. Methods can be attached, but structs don't live in a class hierarchy.

**Rust (2010)**: Structs with ownership semantics. Every struct has a clear owner, and the compiler tracks borrows. This prevents entire categories of bugs but requires more sophisticated analysis.

**Zig (2016)**: Explicit control over layout. You can choose between extern (C-compatible), packed (no padding), or auto (compiler chooses optimal) layout.

Each language's struct design reflects its values and goals.

---

## Type Theory: Products, Records, and Beyond

### Algebraic Data Types

Type theorists think about types algebraically. Just as numbers can be added and multiplied, types can be combined in specific ways.

**Sum Types** (tagged unions, enums): A value is one of several possibilities.
```
type Shape = Circle(radius: f32) | Rectangle(width: f32, height: f32)
```
The number of possible values is the *sum* of the possibilities. If Circle can hold 2³² values and Rectangle can hold 2⁶⁴, Shape can hold 2³² + 2⁶⁴ values.

**Product Types** (structs, tuples): A value contains multiple components.
```
type Point = { x: f32, y: f32 }
```
The number of possible values is the *product*. If x can hold 2³² values and y can hold 2³², Point can hold 2³² × 2³² = 2⁶⁴ values.

This algebraic view explains the names: sum types add possibilities, product types multiply them.

### The Unit and Zero Types

In this algebra:
- **Unit type** (void, ()): Has exactly 1 value. Acts like 1 in multiplication: `T × Unit = T`
- **Never type** (!): Has 0 values. Acts like 0: `T × Never = Never`

A struct with a unit field doesn't increase the number of possible values. A struct with a never field is impossible to construct.

### Nominal vs Structural Typing

This is a fundamental divide in type system design.

**Structural typing**: Types are defined by their structure. Two types with identical structure are the same type.

```typescript
type Point = { x: number, y: number };
type Vector = { x: number, y: number };

function magnitude(p: Point): number { ... }

let v: Vector = { x: 3, y: 4 };
magnitude(v);  // Works! Vector and Point have the same structure
```

TypeScript uses structural typing for objects. This is flexible but can lead to accidental type compatibility.

**Nominal typing**: Types are defined by their declared name. Two types with identical structure but different names are different types.

```rust
struct Point { x: f32, y: f32 }
struct Vector { x: f32, y: f32 }

fn magnitude(p: Point) -> f32 { ... }

let v = Vector { x: 3.0, y: 4.0 };
magnitude(v);  // Error! Vector is not Point
```

Rust uses nominal typing. A Point and a Vector are different types even though they have the same fields. This prevents accidental misuse—you can't pass a "vector" where a "point" is expected—but requires explicit conversions.

### Type Equivalence: A Spectrum

The structural/nominal divide isn't binary. There's a spectrum:

1. **Name equivalence**: Types must have exactly the same name
2. **Declaration equivalence**: Types declared in the same place are equal
3. **Structural equivalence**: Types with the same structure are equal
4. **Behavioral equivalence**: Types that support the same operations are equal

Different languages sit at different points on this spectrum, and some use different rules for different situations.

### Subtyping and Records

When can one type substitute for another? This question of *subtyping* gets interesting with records.

**Width subtyping**: A record with more fields can substitute for one with fewer.
```
{ x: int, y: int, z: int } <: { x: int, y: int }
```
A 3D point can be used where a 2D point is expected (just ignore z). This is called width subtyping because we're making the type "wider."

**Depth subtyping**: Fields can be individually substituted.
```
{ pet: Cat } <: { pet: Animal }  // if Cat <: Animal
```
A record with a more specific field type can substitute for one with a more general field type.

However, subtyping gets tricky with mutation:
```
let point: { x: int, y: int } = { x: 1, y: 2, z: 3 };
point.z = 10;  // Error! { x, y } doesn't have z
```
If we allow width subtyping, we lose the ability to access the extra fields. Languages must choose between flexibility and features.

### Row Polymorphism: The Best of Both Worlds?

Some languages (OCaml, PureScript) support *row polymorphism*, which allows functions to work with records that have "at least" certain fields:

```ocaml
let get_x r = r.x
(* Type: { x: 'a; .. } -> 'a *)
(* Works with any record that has an x field *)
```

The `..` represents "and possibly other fields." This is like bounded parametric polymorphism but for record fields.

Row polymorphism elegantly solves the expression problem for records but requires more sophisticated type inference.

### Records vs Objects

Records and objects seem similar but have fundamental differences:

**Records**:
- Just data
- Fields accessed by name
- No behavior (methods)
- No identity (two records with same values are equivalent)

**Objects**:
- Data + behavior
- Methods accessed by name
- Encapsulation (private fields)
- Identity (two objects can have same values but be different)

Languages blur this distinction. Rust's structs can have methods but aren't objects in the OOP sense. JavaScript's objects are really records with prototype chains.

### Dependent Types and Sized Records

In dependently typed languages, types can depend on values. This enables *length-indexed* types:

```idris
record Vector (n : Nat) (a : Type) where
  elements : Vect n a
```

Here, `n` is a *value* that appears in the *type*. A `Vector 3 Int` is a different type from `Vector 4 Int`. The compiler can prove at compile time that you can't index past the end.

This is exotic for mainstream languages but shows where type theory is heading.

---

## Memory: Where Bits Meet Silicon

Understanding struct layout requires understanding how computers actually access memory.

### The Memory Hierarchy

Modern computers have a hierarchy of memory:

```
       Speed      Size        Cost
       ↑          ↓           ↓
    ┌──────────────────────────┐
    │     CPU Registers        │  < 1 ns, < 1 KB
    ├──────────────────────────┤
    │      L1 Cache            │  ~ 1 ns, 32-64 KB
    ├──────────────────────────┤
    │      L2 Cache            │  ~ 4 ns, 256-512 KB
    ├──────────────────────────┤
    │      L3 Cache            │  ~ 12 ns, 2-32 MB
    ├──────────────────────────┤
    │       Main RAM           │  ~ 60 ns, 8-128 GB
    ├──────────────────────────┤
    │        SSD/Disk          │  ~ 100 µs, TB
    └──────────────────────────┘
```

When your program accesses memory, it doesn't fetch individual bytes. It fetches *cache lines*—typically 64 bytes at a time. If your struct spans two cache lines, accessing it costs twice as much.

### Why Alignment Matters

CPUs are optimized for *aligned* memory access. A 4-byte integer is aligned if its address is divisible by 4. An 8-byte double is aligned if its address is divisible by 8.

Why does this matter? Consider a 4-byte integer at addresses 0-3 vs addresses 1-4:

**Aligned (address 0)**:
```
Memory:  [ int at 0-3  ][ ... ]
Cache:   [     one cache line fetch     ]
```
The CPU fetches one cache line and extracts the integer.

**Unaligned (address 1)**:
```
Memory:  [x][ int at 1-4  ][ ... ]
Cache:   [  line 1  ][  line 2  ]
                 ↑ int spans both!
```
On some architectures, this requires two cache line fetches and a merge. On others (older ARM), it causes a hardware exception!

### The Alignment Rule

Every type has a *natural alignment*:
- `char` / `u8`: 1 byte
- `short` / `i16`: 2 bytes
- `int` / `i32` / `f32`: 4 bytes
- `long` / `i64` / `f64` / pointers: 8 bytes (on 64-bit)
- SIMD types: 16 or 32 bytes

A type should be placed at an address divisible by its alignment.

### Struct Layout Algorithm

Here's how C lays out structs:

```
struct Example {
    char   a;   // 1 byte,  align 1
    int    b;   // 4 bytes, align 4
    char   c;   // 1 byte,  align 1
    double d;   // 8 bytes, align 8
};
```

Step by step:
```
Offset 0: a (1 byte)
Offset 1-3: padding (to align b to 4)
Offset 4: b (4 bytes)
Offset 8: c (1 byte)
Offset 9-15: padding (to align d to 8)
Offset 16: d (8 bytes)
Total: 24 bytes
```

The struct's alignment is the maximum of its fields' alignments (8 in this case). The struct's size is padded to a multiple of its alignment.

### Field Ordering Matters

The same fields in a different order:
```c
struct BetterExample {
    double d;   // 8 bytes, offset 0
    int    b;   // 4 bytes, offset 8
    char   a;   // 1 byte,  offset 12
    char   c;   // 1 byte,  offset 13
    // 2 bytes padding
};  // Total: 16 bytes
```

Same data, 8 bytes smaller! Good compilers can reorder fields, but C guarantees fields appear in declaration order. This is why some languages (Rust, Zig) let you choose:

```rust
#[repr(C)]       // C-compatible layout
#[repr(Rust)]    // Compiler chooses optimal layout
#[repr(packed)]  // No padding (potentially unaligned)
```

### Data-Oriented Design

Game developers and performance engineers think carefully about struct layout. Consider:

```c
// Object-oriented: array of structs
struct Entity {
    Vec3 position;
    Vec3 velocity;
    float health;
    int team;
};
Entity entities[1000];

// Data-oriented: struct of arrays
struct Entities {
    Vec3 positions[1000];
    Vec3 velocities[1000];
    float healths[1000];
    int teams[1000];
};
```

If you're updating all positions, the struct-of-arrays approach is faster because positions are contiguous in memory. Each cache line contains multiple positions, not one position plus velocity plus health plus team.

This is called *structure of arrays* (SoA) vs *array of structures* (AoS). Neither is always better—it depends on access patterns.

### Cache Lines and False Sharing

In multithreaded code, cache lines create subtle issues:

```c
struct Counters {
    int thread1_count;  // Both in same 64-byte cache line!
    int thread2_count;
};
```

If two threads update these counters, they fight over the same cache line, even though they're accessing different variables. This is *false sharing*, and it destroys performance.

Solution: pad to separate cache lines:
```c
struct Counters {
    alignas(64) int thread1_count;
    alignas(64) int thread2_count;
};
```

### Bit Fields and Packed Structs

Sometimes you want precise control over layout:

```c
struct PackedFlags {
    unsigned int flag1 : 1;
    unsigned int flag2 : 1;
    unsigned int flag3 : 1;
    unsigned int value : 5;  // 5 bits, 0-31
};  // Total: 1 byte
```

Bit fields pack multiple values into bytes. This is essential for protocols and hardware registers but is notoriously non-portable—different compilers lay out bit fields differently!

---

## Parsing: Teaching Your Compiler New Syntax

### Extending the Grammar

To support structs, you need to extend your grammar with three new constructs:

```
declaration ::= struct_decl | function_decl | extern_block

struct_decl ::= "struct" IDENT "{" struct_fields "}"
struct_fields ::= (struct_field ",")* struct_field?
struct_field ::= IDENT ":" type

expression ::= ... | struct_literal | field_access
struct_literal ::= IDENT "{" field_inits "}"
field_inits ::= (field_init ",")* field_init?
field_init ::= IDENT ":" expression

field_access ::= expression "." IDENT
```

### The Identifier-Brace Ambiguity

One of the trickiest parsing challenges with structs is distinguishing struct literals from other constructs:

```
foo { x: 1 }
```

This could be:
1. A struct literal: `Foo { x: 1 }`
2. A variable `foo` followed by a block with a labeled statement: `foo; { x: 1 }`
3. A function call with a block argument: `foo({ x: 1 })`

How do languages resolve this?

**Rust**: Struct names must start with uppercase. Also, labeled blocks use `'label:` syntax, eliminating ambiguity.

**Go**: Requires the type in certain contexts. Uses context to disambiguate.

**JavaScript**: Object literals only appear in expression position. Blocks only appear in statement position.

**C**: Uses a different syntax—`(struct Foo){ x, y }` compound literals.

For Som, a reasonable approach is lookahead: when you see an identifier followed by `{`, peek ahead. If you see `IDENT :`, it's a struct literal. Otherwise, it's a variable followed by something else.

### Precedence and Field Access

Field access (`.`) needs the highest precedence of any postfix operator:

```
point.x + point.y     // (point.x) + (point.y)
points[0].x           // (points[0]).x
make_point().x        // (make_point()).x
point.offset.x.abs()  // (((point.offset).x).abs())
```

In a Pratt parser, you'd handle `.` in the postfix/infix loop with a very high left binding power (say, 18-20):

```rust
loop {
    // Check for field access FIRST (highest precedence)
    if self.at(TokenKind::Dot) {
        self.advance();
        let field = self.parse_ident()?;
        lhs = Expr::FieldAccess { object: lhs, field };
        continue;
    }

    // Then function calls
    if self.at(TokenKind::OpenParen) { ... }

    // Then infix operators
    let op = self.peek_binop()?;
    let (l_bp, r_bp) = op.binding_power();
    if l_bp < min_bp { break; }
    ...
}
```

### Expression-Oriented Design

An interesting design question: is a struct literal an expression or a special form?

**Expression-oriented** (Rust, functional languages):
```rust
let p = if condition { Point { x: 1, y: 2 } } else { Point { x: 3, y: 4 } };
```
Struct literals are just expressions. They can appear anywhere an expression can.

**Statement-oriented** (C):
```c
struct Point p;
if (condition) {
    p = (struct Point){ 1, 2 };
} else {
    p = (struct Point){ 3, 4 };
}
```
C's compound literals are expressions but feel awkward. Struct initialization is traditionally statement-oriented.

Expression-oriented designs are more composable but require thinking about struct literals as values that can flow through your program.

---

## Type Checking: Ensuring Correctness

### What the Type Checker Must Verify

For struct declarations:
1. Struct name is unique
2. All field names within a struct are unique
3. All field types exist and are valid

For struct literals:
1. The struct type exists
2. All required fields are provided
3. No unknown fields are specified
4. Each field value matches the expected type
5. No duplicate fields

For field access:
1. The expression being accessed has a struct type
2. The field exists on that struct
3. Return the field's type

### Implementing Struct Type Checking

Let's trace through type checking a struct literal:

```
let p = Vec2 { x: 1.0, y: 2.0 };
```

```rust
fn check_struct_literal(&mut self, name: &str, fields: &[(Ident, ExprId)]) -> Type {
    // 1. Look up the struct definition
    let def_id = match self.struct_registry.get(name) {
        Some(id) => *id,
        None => {
            self.error(UnknownStruct { name });
            return Type::Error;
        }
    };

    let struct_def = self.ast.get_struct(def_id);

    // 2. Track which fields we've seen
    let mut seen_fields = HashSet::new();

    // 3. Check each provided field
    for (field_name, value_expr) in fields {
        // Duplicate field check
        if !seen_fields.insert(&field_name.value) {
            self.error(DuplicateField { field: field_name });
            continue;
        }

        // Find the expected field
        let expected_field = struct_def.fields.iter()
            .find(|f| f.name.value == field_name.value);

        match expected_field {
            Some(field_def) => {
                // Check the value matches the expected type
                let actual_type = self.infer(value_expr);
                self.unify(&field_def.ty, &actual_type)?;
            }
            None => {
                self.error(UnknownField {
                    struct_name: name,
                    field: field_name,
                    available: struct_def.fields.iter().map(|f| &f.name.value),
                });
            }
        }
    }

    // 4. Check all required fields are present
    for field_def in &struct_def.fields {
        if !seen_fields.contains(&field_def.name.value) {
            self.error(MissingField {
                struct_name: name,
                field: &field_def.name,
            });
        }
    }

    Type::Struct { name: name.into(), def_id }
}
```

### Unification with Struct Types

When your type inferencer encounters constraints involving structs:

```rust
fn unify(&mut self, expected: &Type, actual: &Type) -> Result<()> {
    match (expected, actual) {
        (Type::Struct { def_id: d1, .. }, Type::Struct { def_id: d2, .. }) => {
            if d1 == d2 {
                Ok(())  // Same struct type
            } else {
                Err(TypeMismatch { expected, actual })
            }
        }
        // ... other cases
    }
}
```

With nominal typing, unification is simple: two struct types are equal if and only if they're the same struct (same def_id).

### Type Inference Challenges

Struct literals interact with type inference in interesting ways:

```rust
let p = Point { x: 1, y: 2 };  // What type is 1?
```

If your language has multiple numeric types (i32, i64, f32, f64), you need to decide:
1. Use the field's declared type to constrain the literal
2. Infer a default type for the literal, then check compatibility

Most languages do (1): the struct field type "flows backward" to constrain the literal.

```rust
struct Point { x: f32, y: f32 }
let p = Point { x: 1, y: 2 };  // 1 and 2 inferred as f32
```

This requires your inferencer to propagate expected types downward, not just actual types upward.

---

## Code Generation: From Abstract to Concrete

### The Challenge of Aggregates

Most compiler IRs (LLVM IR, Cranelift IR) don't have first-class struct types in the way high-level languages do. They work with primitive values: integers, floats, pointers.

This means you must *lower* struct operations into primitive operations:
- Struct creation → allocate memory, store fields
- Field access → compute address, load value
- Passing structs → depends on ABI

### Stack Allocation

When you create a local struct variable:

```
let p = Point { x: 1.0, y: 2.0 };
```

You typically:
1. Allocate space on the stack
2. Store each field at its computed offset

In Cranelift:
```rust
// Calculate layout
let layout = compute_layout(&struct_def);  // size: 8, align: 4

// Create stack slot
let slot = func.create_sized_stack_slot(StackSlotData::new(
    StackSlotKind::ExplicitSlot,
    layout.size as u32,
    layout.align as u8,
));

// Get base address
let base = func.ins().stack_addr(types::I64, slot, 0);

// Store x at offset 0
let x_val = func.ins().f32const(1.0);
func.ins().store(MemFlags::new(), x_val, base, 0);

// Store y at offset 4
let y_val = func.ins().f32const(2.0);
func.ins().store(MemFlags::new(), y_val, base, 4);
```

### Field Access

Reading a field:
```
let x = p.x;
```

Generates:
```rust
// Get base address of p
let base = /* ... */;

// Load from offset 0 (where x lives)
let x_val = func.ins().load(types::F32, MemFlags::new(), base, 0);
```

Writing a field:
```
p.x = 5.0;
```

Generates:
```rust
let base = /* ... */;
let new_val = func.ins().f32const(5.0);
func.ins().store(MemFlags::new(), new_val, base, 0);
```

### SROA: Scalar Replacement of Aggregates

A crucial optimization: if a struct is small and never has its address taken, why bother with memory at all?

```
let p = Point { x: 1.0, y: 2.0 };
let sum = p.x + p.y;
```

An optimizing compiler can transform this to:
```
let p_x = 1.0;
let p_y = 2.0;
let sum = p_x + p_y;
```

No memory allocation needed! The struct exists only as separate scalar values in registers.

This is called *Scalar Replacement of Aggregates* (SROA) and is one of the most important optimizations for structs. LLVM does this aggressively. Cranelift, being a simpler compiler, relies more on the frontend to produce good code.

### The "What Is a Struct Value?" Question

This is subtle but important. When you evaluate a struct expression, what do you get?

**Option 1: Always a pointer**
```rust
fn gen_struct_literal(...) -> Value {
    let slot = allocate_stack_slot(...);
    store_fields(slot, ...);
    return slot;  // Return the address
}
```
Simple, but means all struct operations involve memory access.

**Option 2: Multiple values (small structs)**
```rust
fn gen_struct_literal(...) -> Vec<Value> {
    vec![
        func.ins().f32const(1.0),  // x
        func.ins().f32const(2.0),  // y
    ]  // Return multiple values
}
```
For small structs, keep them "exploded" as separate values. Only combine into memory when necessary (taking address, passing to memory-expecting code).

Most real compilers use a hybrid: small structs stay in registers, large structs go to memory. The ABI determines the threshold.

### Nested Structs

```
struct Line { start: Point, end: Point }
```

Nested structs are flattened in memory:
```
Offset 0: start.x
Offset 4: start.y
Offset 8: end.x
Offset 12: end.y
Total: 16 bytes
```

Field access chains work by accumulating offsets:
```
line.start.x     // offset 0
line.end.y       // offset 8 + 4 = 12
```

---

## The ABI: Where Compilers Must Agree

### What Is an ABI?

The Application Binary Interface is a contract between compiled code:
- How arguments are passed to functions
- How return values are delivered
- Which registers are preserved across calls
- How the stack is managed
- How structs are laid out in memory

When you call a function compiled by a different compiler (or language), both must agree on the ABI. Get it wrong, and you'll see:
- Garbage values for arguments
- Crashes from stack corruption
- Silent data corruption

### A Brief History of Calling Conventions

**Early days**: Everything on the stack. Simple, portable, slow.

**x86 chaos**: The 32-bit x86 era had a dozen conventions:
- cdecl (C default): Caller cleans stack
- stdcall (Windows API): Callee cleans stack
- fastcall: First two args in registers
- thiscall: `this` pointer in ECX
- And more...

**x86-64 unification**: 64-bit brought sanity. Two main conventions:
- System V AMD64 ABI (Linux, macOS, BSD): RDI, RSI, RDX, RCX, R8, R9 for integers; XMM0-7 for floats
- Microsoft x64 (Windows): RCX, RDX, R8, R9 for integers; XMM0-3 for floats

**ARM64**: Simpler than x86. X0-X7 for integers, V0-V7 for floats. Designed from scratch with modern needs.

### How Structs Are Passed

This is where it gets complicated. The rules differ by ABI, struct size, and field types.

#### x86-64 System V (Linux/macOS)

The SysV ABI classifies each 8-byte "eightbyte" of a struct:

1. **INTEGER**: Contains integer/pointer types
2. **SSE**: Contains float/double types
3. **MEMORY**: Too complex, must pass on stack

**Small structs (≤ 16 bytes, ≤ 2 eightbytes)**:
- Classify each eightbyte
- INTEGER → pass in RDI/RSI/RDX/RCX/R8/R9
- SSE → pass in XMM0-7
- If either eightbyte is MEMORY, whole struct is MEMORY

**Large structs (> 16 bytes)**:
- Always passed on the stack
- Actually, caller allocates space, passes pointer as hidden first arg

**Example: Vec2 (8 bytes, 2 floats)**:
```c
struct Vec2 { float x; float y; };
void print_vec2(Vec2 v);
```
- Vec2 is 8 bytes, one eightbyte
- Contains floats → SSE
- Passed in XMM0

**Example: Color (4 bytes, 4 u8s)**:
```c
struct Color { uint8_t r, g, b, a; };
void print_color(Color c);
```
- Color is 4 bytes, one eightbyte
- Contains integers → INTEGER
- Passed in RDI (low 4 bytes)

**Example: Mixed (12 bytes)**:
```c
struct Mixed { float f; long l; };
void use_mixed(Mixed m);
```
- Mixed is 12 bytes, two eightbytes
- First eightbyte (f + padding): SSE
- Second eightbyte (l): INTEGER
- f in XMM0, l in RDI

#### ARM64

ARM64 has similar concepts but different details:

- Up to 8 integer registers (X0-X7)
- Up to 8 SIMD registers (V0-V7)
- *Homogeneous Floating-Point Aggregates* (HFA): Structs of 1-4 floats/doubles go in consecutive V registers
- Other small structs (≤ 16 bytes) are split across X registers

**Example: Vec2 on ARM64**:
```c
struct Vec2 { float x; float y; };
```
- Two floats → HFA
- Passed in S0 and S1 (low 32 bits of V0 and V1)

This is different from x86-64! The same struct is passed differently on different platforms.

### Struct Returns

Returning structs is similar but reversed:

**Small structs**: Returned in registers (RAX/RDX or XMM0/XMM1 on x86-64)

**Large structs**: Caller passes a "secret" pointer where the return value should be written. This is called *sret* (struct return).

```c
// Source
Big make_big(void);

// What actually happens (conceptual)
void make_big(Big* __sret);
```

The sret pointer is typically the first argument (before any explicit arguments).

### Implementing ABI Classification

Here's a simplified classifier for x86-64 SysV:

```rust
enum Class {
    Integer,
    Sse,
    Memory,
}

fn classify_struct(layout: &StructLayout, fields: &[Type]) -> Vec<Class> {
    // Large structs → MEMORY
    if layout.size > 16 {
        return vec![Class::Memory];
    }

    let mut classes = vec![];

    // Classify each 8-byte chunk
    for chunk_start in (0..layout.size).step_by(8) {
        let chunk_end = (chunk_start + 8).min(layout.size);

        // What fields overlap this chunk?
        let mut has_float = false;
        let mut has_int = false;

        for (i, field) in fields.iter().enumerate() {
            let field_start = layout.offsets[i];
            let field_end = field_start + field.size();

            if field_start < chunk_end && field_end > chunk_start {
                match field {
                    Type::F32 | Type::F64 => has_float = true,
                    _ => has_int = true,
                }
            }
        }

        // INTEGER beats SSE (mixed → INTEGER)
        if has_int {
            classes.push(Class::Integer);
        } else {
            classes.push(Class::Sse);
        }
    }

    classes
}
```

Real implementations are more complex, handling:
- Arrays
- Nested structs
- Alignment requirements
- Post-merger rules

### Building Correct Function Signatures

For FFI to work, your function signatures must match what C expects:

```rust
fn build_signature(&self, params: &[Type], ret: &Type) -> Signature {
    let mut sig = Signature::new(CallConv::SystemV);

    // Handle struct return
    let ret_layout = self.get_layout(ret);
    if needs_sret(&ret_layout) {
        sig.params.push(AbiParam::special(
            types::I64,
            ArgumentPurpose::StructReturn,
        ));
    }

    // Handle each parameter
    for param in params {
        let layout = self.get_layout(param);
        let classes = classify_struct(&layout);

        match classes.as_slice() {
            [Class::Memory] => {
                // Large struct: pass pointer
                sig.params.push(AbiParam::new(types::I64));
            }
            classes => {
                // Small struct: split into registers
                for (i, class) in classes.iter().enumerate() {
                    let ty = match class {
                        Class::Integer => types::I64,
                        Class::Sse => types::F64,  // Simplification
                        Class::Memory => unreachable!(),
                    };
                    sig.params.push(AbiParam::new(ty));
                }
            }
        }
    }

    // Handle return type
    // ... similar logic ...

    sig
}
```

---

## Advanced Topics: Where Things Get Interesting

### Move Semantics and Ownership

In Rust, structs interact deeply with ownership:

```rust
struct File { handle: RawHandle }

let f1 = File::open("test.txt")?;
let f2 = f1;  // f1 is MOVED to f2
// f1 is no longer valid!
```

When a struct is moved:
- Its bytes are copied
- The source becomes invalid
- Destructors only run on the destination

This requires tracking which variables are "live" and preventing use after move. Your borrow checker needs to understand struct moves.

### Copy vs Move

Some types are `Copy` (can be freely duplicated):
```rust
#[derive(Copy)]
struct Point { x: f32, y: f32 }

let p1 = Point { x: 1.0, y: 2.0 };
let p2 = p1;  // Copies, p1 still valid
```

Others are move-only:
```rust
struct File { handle: RawHandle }

let f1 = File::open(...)?;
let f2 = f1;  // Moves, f1 invalid
```

The distinction usually depends on whether the type "owns" a resource (memory, file handle, etc.).

### Interior Mutability

How do you mutate a field without a mutable reference?

```rust
struct Counter {
    count: Cell<u32>,  // Interior mutability!
}

impl Counter {
    fn increment(&self) {  // Note: &self, not &mut self
        self.count.set(self.count.get() + 1);
    }
}
```

`Cell` and similar types use unsafe code internally to enable mutation through shared references. This is essential for certain patterns but requires careful API design.

### Zero-Sized Types

Some structs have size 0:
```rust
struct Unit;
struct Empty {}
struct Phantom<T>(PhantomData<T>);
```

These are useful for:
- Type-level markers
- Phantom type parameters
- Zero-cost abstraction

Crucially, arrays of ZSTs take no memory, and references to ZSTs don't need valid addresses. Your compiler should handle these gracefully.

### Self-Referential Structs

What if a struct contains a reference to itself?

```rust
struct SelfRef {
    value: String,
    ref_to_value: &str,  // Points into value!
}
```

This is notoriously difficult because moving the struct invalidates the reference. Languages handle this differently:
- **Rust**: Pin API prevents moving pinned types
- **C++**: Move constructors update internal pointers
- **Most languages**: Just don't allow it

### Generic Structs

Structs with type parameters:
```rust
struct Pair<T, U> {
    first: T,
    second: U,
}
```

These require *monomorphization*: generating separate code for each concrete type. `Pair<i32, i32>` and `Pair<String, bool>` become different types with different layouts.

Alternatively, *type erasure* uses a uniform representation (pointers) and avoids code duplication at the cost of indirection.

### Dynamically Sized Types

What if a struct's size isn't known at compile time?

```rust
struct Slice<T> {
    len: usize,
    data: [T],  // Dynamically sized!
}
```

These can only exist behind pointers, and the pointer carries extra information (length). Rust calls these *fat pointers*.

---

## Implementation Walkthrough

Now let's apply everything we've learned to actually implement structs.

### Step 1: Extend the Type System

**File: `src/ast/ty.rs`**

Add new types:
```rust
pub enum Type {
    // Existing types...
    Unit,
    I32,
    Bool,
    Str,

    // New primitive types for FFI
    U8,
    F32,
    F64,

    // The struct type
    Struct {
        name: Box<str>,
        def_id: Id<StructDef>,
    },

    // Existing...
    Reference { ... },
    Fun { ... },
}
```

Update Display:
```rust
impl Display for Type {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Type::U8 => write!(f, "u8"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::Struct { name, .. } => write!(f, "{}", name),
            // ...
        }
    }
}
```

### Step 2: Define Struct AST Nodes

**File: `src/ast/decl.rs`**

```rust
/// A struct definition
pub struct StructDef {
    pub name: Ident,
    pub fields: Vec<StructField>,
}

/// A field in a struct definition
pub struct StructField {
    pub name: Ident,
    pub ty: Type,
}
```

**File: `src/ast/expr.rs`**

```rust
pub enum Expr {
    // Existing variants...

    /// Struct literal: `Point { x: 1.0, y: 2.0 }`
    StructLiteral {
        struct_name: Ident,
        fields: Vec<StructFieldInit>,
    },

    /// Field access: `point.x`
    FieldAccess {
        object: Id<Expr>,
        field: Ident,
    },
}

/// A field initializer in a struct literal
pub struct StructFieldInit {
    pub name: Ident,
    pub value: Id<Expr>,
}
```

### Step 3: Add Storage to AST

**File: `src/ast/mod.rs`**

```rust
pub struct Ast {
    // Existing...
    pub exprs: Arena<Expr>,
    pub stmts: Arena<Stmt>,
    pub funcs: Arena<Func>,

    // New
    pub struct_defs: Arena<StructDef>,
    struct_def_spans: HashMap<Id<StructDef>, Span>,
    pub struct_registry: HashMap<String, Id<StructDef>>,
}

impl Ast {
    pub fn alloc_struct_def(&mut self, def: StructDef) -> Id<StructDef> {
        let id = self.struct_defs.alloc(def);
        self.struct_registry.insert(
            self.struct_defs.get(&id).name.value.to_string(),
            id,
        );
        id
    }

    pub fn alloc_struct_def_with_span(
        &mut self,
        def: StructDef,
        span: Span,
    ) -> Id<StructDef> {
        let id = self.alloc_struct_def(def);
        self.struct_def_spans.insert(id, span);
        id
    }
}
```

### Step 4: Extend the Lexer

**File: `src/lexer/mod.rs`**

```rust
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // Existing tokens...

    #[token("struct")]
    Struct,

    #[token(".")]
    Dot,

    // Note: F32, F64, U8 already exist!
}
```

### Step 5: Parse Struct Declarations

**File: `src/parser/decl.rs`**

Add to `parse_program`:
```rust
pub(super) fn parse_program(&mut self) {
    while !self.at_eof() {
        if self.at(TokenKind::Fn) {
            self.parse_func();
        } else if self.at(TokenKind::Extern) {
            self.parse_extern_block();
        } else if self.at(TokenKind::Struct) {
            self.parse_struct_decl();
        } else {
            self.error(vec![TokenKind::Fn, TokenKind::Extern, TokenKind::Struct]);
            self.advance();
        }
    }
}
```

Add struct parsing:
```rust
fn parse_struct_decl(&mut self) -> Option<Id<StructDef>> {
    let start = self.peek_span();

    self.expect(TokenKind::Struct)?;
    let (name, _) = self.parse_ident()?;
    self.expect(TokenKind::OpenBrace)?;

    let mut fields = Vec::new();

    while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
        let (field_name, _) = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let field_ty = self.parse_type()?;

        fields.push(StructField {
            name: field_name,
            ty: field_ty,
        });

        // Allow trailing comma
        if !self.at(TokenKind::CloseBrace) {
            self.expect(TokenKind::Comma)?;
        }
    }

    self.expect(TokenKind::CloseBrace)?;

    let span = start.merge(&self.previous_span());
    let def = StructDef { name, fields };

    Some(self.ast.alloc_struct_def_with_span(def, span))
}
```

### Step 6: Parse Type Keywords

**File: `src/parser/ty.rs`**

```rust
pub fn parse_type(&mut self) -> Option<Type> {
    match self.peek() {
        TokenKind::I32 => { self.advance(); Some(Type::I32) }
        TokenKind::Bool => { self.advance(); Some(Type::Bool) }
        TokenKind::Str => { self.advance(); Some(Type::Str) }

        // New types
        TokenKind::U8 => { self.advance(); Some(Type::U8) }
        TokenKind::F32 => { self.advance(); Some(Type::F32) }
        TokenKind::F64 => { self.advance(); Some(Type::F64) }

        TokenKind::Ident => {
            // Could be a struct type or type parameter
            let name = self.peek_token().text;
            self.advance();

            // Look up in struct registry
            if let Some(def_id) = self.ast.struct_registry.get(name) {
                Some(Type::Struct {
                    name: name.into(),
                    def_id: *def_id,
                })
            } else {
                // Assume type parameter for now
                Some(Type::Named(name.into()))
            }
        }

        // ... references, etc.
    }
}
```

### Step 7: Parse Struct Literals and Field Access

**File: `src/parser/expr.rs`**

Modify `parse_atom` for struct literals:
```rust
fn parse_atom(&mut self) -> Option<Id<Expr>> {
    match self.peek() {
        TokenKind::Ident => {
            let token = self.peek_token();
            let name = token.text;
            let span = token.span.clone();
            self.advance();

            // Check for struct literal: Name { ... }
            if self.at(TokenKind::OpenBrace) {
                // Peek ahead to disambiguate from block
                if self.looks_like_struct_literal() {
                    return self.parse_struct_literal(name, span);
                }
            }

            // Otherwise it's a variable
            let ident = self.make_ident(name);
            Some(self.ast.alloc_expr_with_span(Expr::Var(ident), span))
        }
        // ... other cases
    }
}

fn looks_like_struct_literal(&self) -> bool {
    // Peek: { ident :
    // This is a heuristic
    self.at(TokenKind::OpenBrace)
        && self.peek_n(1).kind == TokenKind::Ident
        && self.peek_n(2).kind == TokenKind::Colon
}

fn parse_struct_literal(&mut self, name: &str, start: Span) -> Option<Id<Expr>> {
    self.expect(TokenKind::OpenBrace)?;

    let mut fields = Vec::new();

    while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
        let (field_name, _) = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let value = self.parse_expr()?;

        fields.push(StructFieldInit {
            name: field_name,
            value,
        });

        if !self.at(TokenKind::CloseBrace) {
            self.expect(TokenKind::Comma)?;
        }
    }

    self.expect(TokenKind::CloseBrace)?;

    let span = start.merge(&self.previous_span());
    Some(self.ast.alloc_expr_with_span(
        Expr::StructLiteral {
            struct_name: self.make_ident(name),
            fields,
        },
        span,
    ))
}
```

Add field access in `parse_expr_bp`:
```rust
fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
    let start_span = self.peek_span();
    let mut lhs = self.parse_atom()?;

    loop {
        // Field access - highest precedence
        if self.at(TokenKind::Dot) {
            self.advance();
            let (field, _) = self.parse_ident()?;
            let span = start_span.merge(&self.previous_span());
            lhs = self.ast.alloc_expr_with_span(
                Expr::FieldAccess { object: lhs, field },
                span,
            );
            continue;
        }

        // Function calls
        if self.at(TokenKind::OpenParen) { ... }

        // Binary operators
        // ...
    }
}
```

### Step 8: Type Check Structs

**File: `src/type_check/mod.rs`**

Add struct literal inference:
```rust
fn infer(&mut self, ast: &Ast, expr_id: &Id<Expr>) -> Type {
    let expr = ast.exprs.get(expr_id);

    match expr {
        // ... existing cases ...

        Expr::StructLiteral { struct_name, fields } => {
            let def_id = match ast.struct_registry.get(&*struct_name.value) {
                Some(id) => *id,
                None => {
                    self.errors.push(TypeError::UnknownStruct {
                        name: struct_name.value.clone(),
                        span: ast.get_expr_span(expr_id),
                    });
                    return Type::Unknown(self.fresh_var());
                }
            };

            let struct_def = ast.struct_defs.get(&def_id);

            // Check each field
            for init in fields {
                let field_def = struct_def.fields.iter()
                    .find(|f| f.name.value == init.name.value);

                match field_def {
                    Some(f) => {
                        let actual = self.infer(ast, &init.value);
                        self.constrain(actual, f.ty.clone());
                    }
                    None => {
                        self.errors.push(TypeError::UnknownField { ... });
                    }
                }
            }

            // Check all fields provided
            for f in &struct_def.fields {
                if !fields.iter().any(|init| init.name.value == f.name.value) {
                    self.errors.push(TypeError::MissingField { ... });
                }
            }

            Type::Struct {
                name: struct_name.value.clone(),
                def_id,
            }
        }

        Expr::FieldAccess { object, field } => {
            let obj_ty = self.infer(ast, object);
            let obj_ty = self.resolve(&obj_ty);

            match obj_ty {
                Type::Struct { def_id, .. } => {
                    let struct_def = ast.struct_defs.get(&def_id);
                    let field_def = struct_def.fields.iter()
                        .find(|f| f.name.value == field.value);

                    match field_def {
                        Some(f) => f.ty.clone(),
                        None => {
                            self.errors.push(TypeError::UnknownField { ... });
                            Type::Unknown(self.fresh_var())
                        }
                    }
                }
                _ => {
                    self.errors.push(TypeError::FieldAccessOnNonStruct { ... });
                    Type::Unknown(self.fresh_var())
                }
            }
        }
    }
}
```

### Step 9: Code Generation

**File: `src/code_gen/mod.rs`**

First, add primitive type mapping:
```rust
pub fn to_type(ty: &Type) -> cranelift::prelude::Type {
    match ty {
        Type::U8 => types::I8,
        Type::I32 => types::I32,
        Type::F32 => types::F32,
        Type::F64 => types::F64,
        Type::Bool => types::I8,
        Type::Str => types::I64,  // pointer
        Type::Struct { .. } => {
            panic!("Structs must be handled specially, not mapped to single type")
        }
        // ...
    }
}
```

Add layout computation:
```rust
struct StructLayout {
    size: u32,
    align: u8,
    field_offsets: Vec<u32>,
}

fn compute_layout(struct_def: &StructDef) -> StructLayout {
    let mut offset = 0u32;
    let mut max_align = 1u8;
    let mut offsets = Vec::new();

    for field in &struct_def.fields {
        let (size, align) = type_size_align(&field.ty);
        max_align = max_align.max(align);

        // Align offset
        let align = align as u32;
        offset = (offset + align - 1) & !(align - 1);

        offsets.push(offset);
        offset += size;
    }

    // Pad to struct alignment
    let align = max_align as u32;
    let size = (offset + align - 1) & !(align - 1);

    StructLayout {
        size,
        align: max_align,
        field_offsets: offsets,
    }
}

fn type_size_align(ty: &Type) -> (u32, u8) {
    match ty {
        Type::U8 | Type::Bool => (1, 1),
        Type::I32 | Type::F32 => (4, 4),
        Type::F64 => (8, 8),
        Type::Str => (8, 8),  // pointer
        Type::Struct { def_id, .. } => {
            // Recursive layout computation
            let struct_def = /* get def */;
            let layout = compute_layout(struct_def);
            (layout.size, layout.align)
        }
        _ => (8, 8),  // default to pointer size
    }
}
```

Generate struct literals:
```rust
fn gen_expr(&mut self, func: &mut FuncCtx, expr_id: Id<Expr>) -> Value {
    match expr {
        Expr::StructLiteral { struct_name, fields } => {
            let def_id = self.ast.struct_registry.get(&*struct_name.value).unwrap();
            let struct_def = self.ast.struct_defs.get(def_id);
            let layout = compute_layout(struct_def);

            // Allocate stack space
            let slot = func.body.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                layout.size,
                layout.align,
            ));

            let base = func.body.ins().stack_addr(types::I64, slot, 0);

            // Store each field
            for init in fields {
                let idx = struct_def.fields.iter()
                    .position(|f| f.name.value == init.name.value)
                    .unwrap();

                let offset = layout.field_offsets[idx] as i32;
                let value = self.gen_expr(func, init.value);

                func.body.ins().store(
                    MemFlags::new(),
                    value,
                    base,
                    offset,
                );
            }

            base  // Return the address
        }

        Expr::FieldAccess { object, field } => {
            let obj_ty = self.get_type(object);
            let Type::Struct { def_id, .. } = obj_ty else { panic!() };

            let struct_def = self.ast.struct_defs.get(&def_id);
            let layout = compute_layout(struct_def);

            let idx = struct_def.fields.iter()
                .position(|f| f.name.value == field.value)
                .unwrap();

            let offset = layout.field_offsets[idx] as i32;
            let field_ty = &struct_def.fields[idx].ty;

            let base = self.gen_expr(func, *object);
            func.body.ins().load(
                to_type(field_ty),
                MemFlags::new(),
                base,
                offset,
            )
        }

        // ... other cases
    }
}
```

---

## Testing Strategy

### Incremental Testing

Don't try to implement everything at once. Build up functionality test by test:

**Test 1: Parse struct definition**
```
struct Vec2 { x: f32, y: f32 }
fn main() -> i32 { 0 }
```
Verify the parser accepts the syntax.

**Test 2: Struct literal (no FFI)**
```
struct Vec2 { x: f32, y: f32 }
fn main() -> i32 {
    let p = Vec2 { x: 1.0, y: 2.0 };
    0
}
```
Verify type checking and basic codegen.

**Test 3: Field access**
```
fn main() -> i32 {
    let p = Vec2 { x: 1.0, y: 2.0 };
    let x = p.x;
    0
}
```

**Test 4: Pass struct to C**
```
extern "test_ffi.so" { fn print_vec2(v: Vec2); }

fn main() -> i32 {
    print_vec2(Vec2 { x: 1.0, y: 2.0 });
    0
}
```
Now you're testing ABI correctness.

**Test 5: Return struct from C**
```
extern "test_ffi.so" { fn make_vec2(x: f32, y: f32) -> Vec2; }

fn main() -> i32 {
    let v = make_vec2(3.0, 4.0);
    0
}
```

**Test 6: Round-trip**
```
fn main() -> i32 {
    print_vec2(make_vec2(5.0, 6.0));
    0
}
```
If this prints `Vec2(5.0, 6.0)`, your ABI is correct!

**Test 7: Small integer struct**
```
struct Color { r: u8, g: u8, b: u8, a: u8 }
extern "test_ffi.so" { fn print_color(c: Color); }

fn main() -> i32 {
    print_color(Color { r: 255, g: 128, b: 64, a: 255 });
    0
}
```

**Test 8: Large struct (sret)**
```
struct Big { x: f32, y: f32, z: f32, w: f32, v: f32 }
extern "test_ffi.so" { fn print_big(b: Big); }

fn main() -> i32 {
    print_big(Big { x: 1.0, y: 2.0, z: 3.0, w: 4.0, v: 5.0 });
    0
}
```

### The C Test Library

**File: `tests/test_ffi.c`**

```c
#include <stdio.h>
#include <stdint.h>

typedef struct { float x; float y; } Vec2;
typedef struct { uint8_t r; uint8_t g; uint8_t b; uint8_t a; } Color;
typedef struct { float x; float y; float z; float w; float v; } Big;

int32_t identity(int32_t x) { return x; }

Vec2 make_vec2(float x, float y) {
    return (Vec2){ x, y };
}

void print_vec2(Vec2 v) {
    printf("Vec2(%f, %f)\n", v.x, v.y);
}

Vec2 add_vec2(Vec2 a, Vec2 b) {
    return (Vec2){ a.x + b.x, a.y + b.y };
}

void print_color(Color c) {
    printf("Color(%d, %d, %d, %d)\n", c.r, c.g, c.b, c.a);
}

void print_big(Big b) {
    printf("Big(%f, %f, %f, %f, %f)\n", b.x, b.y, b.z, b.w, b.v);
}
```

Compile:
```bash
# Linux
gcc -shared -fPIC -o tests/test_ffi.so tests/test_ffi.c

# macOS (already compiled as test_ffi.so in your repo)
clang -shared -o tests/test_ffi.so tests/test_ffi.c
```

### Debugging ABI Issues

When struct passing doesn't work:

1. **Print the Cranelift IR**: See exactly what code is generated
2. **Use a debugger**: Set breakpoints in the C code, examine register contents
3. **Start smaller**: If Vec2 fails, try a struct with one field
4. **Check alignment**: Print `sizeof` and `alignof` in C, compare to your layout
5. **Try both directions**: If passing works but returning doesn't, the problem is return handling

---

## Further Reading

### Books

**Compilers: Principles, Techniques, and Tools** (The Dragon Book)
*Aho, Lam, Sethi, Ullman*
The classic compiler textbook. Heavy but comprehensive.

**Types and Programming Languages**
*Benjamin C. Pierce*
The bible of type theory. Covers products, sums, records, subtyping, and much more.

**Crafting Interpreters**
*Robert Nystrom*
Practical and accessible. Free online. Great for learning by doing.

**Engineering a Compiler**
*Cooper and Torczon*
More modern than the Dragon Book, with better coverage of optimization.

### Papers

**"A Tutorial Implementation of a Dependently Typed Lambda Calculus"**
*Andres Löh, Conor McBride, Wouter Swierstra*
Shows how to implement a type system with dependent types.

**"The Essence of Compiling with Continuations"**
*Cormac Flanagan et al.*
Influential paper on compiler intermediate representations.

### ABI Documentation

**System V AMD64 ABI**
https://gitlab.com/x86-psABIs/x86-64-ABI
The definitive x86-64 calling convention specification.

**ARM64 ABI**
https://github.com/ARM-software/abi-aa
ARM's official ABI documentation.

**Agner Fog's Calling Conventions**
https://www.agner.org/optimize/calling_conventions.pdf
Excellent practical guide covering multiple platforms.

### Related Projects

**Cranelift**
https://cranelift.dev/
Your code generator. Read the docs and examples.

**Rust Compiler (rustc)**
https://github.com/rust-lang/rust
See how a real compiler handles structs. Look in `compiler/rustc_target/src/abi/`.

**Zig Compiler**
https://github.com/ziglang/zig
Clean codebase with explicit ABI handling.

**QBE**
https://c9x.me/compile/
A small compiler backend, simpler than LLVM/Cranelift. Good for understanding.

### Online Resources

**Compiler Explorer (Godbolt)**
https://godbolt.org/
See how different compilers handle structs. Invaluable for understanding ABI.

**"Demystifying Programs"** blog series
https://blog.pnkfx.org/
Deep dives into compiler implementation topics.

---

## Closing Thoughts

Implementing structs is a journey through the entire compiler stack. You'll touch parsing, type checking, memory layout, code generation, and ABI compatibility. Each piece teaches fundamental concepts that apply far beyond this single feature.

Take it one step at a time:
1. Get parsing working first
2. Then type checking
3. Then basic codegen (no FFI)
4. Finally, tackle the ABI

When you're debugging ABI issues at 2 AM, remember: every compiler engineer has been there. The x86-64 SysV ABI document is your friend. Godbolt is your friend. Print statements in your codegen are your friend.

And when you finally see `Vec2(5.0, 6.0)` printed correctly by your C test function—called from code your compiler generated—you'll understand why people build compilers.

Good luck!

# Multiple Dispatch Implementation Plan for Som

## Overview

This document outlines a plan to add CLOS-style multiple dispatch to the Som programming language. Multiple dispatch allows functions to have different implementations based on the types of **all** their arguments (not just the first one like traditional OOP).

## Table of Contents

1. [Motivation](#motivation)
2. [Syntax Design](#syntax-design)
3. [Type System Changes](#type-system-changes)
4. [Dispatch Algorithm](#dispatch-algorithm)
5. [Parser Modifications](#parser-modifications)
6. [Type Checker Updates](#type-checker-updates)
7. [Code Generation Strategy](#code-generation-strategy)
8. [Implementation Phases](#implementation-phases)
9. [Edge Cases & Error Handling](#edge-cases--error-handling)
10. [Examples](#examples)

---

## Motivation

### Problems Multiple Dispatch Solves

**1. Symmetric Binary Operations**
Operations between two different types where neither is "primary":
```som
// Without multiple dispatch - must pick which type "owns" the operation
impl Asteroid {
    fn collide_with_spaceship(self, s ~ Spaceship) { ... }
};

// With multiple dispatch - symmetric and natural
impl fn collide(a ~ Asteroid, b ~ Spaceship) { ... };
impl fn collide(a ~ Spaceship, b ~ Asteroid) { ... };
```

**2. The Expression Problem**
Adding new operations to existing types without modifying them:
```som
// Later, in a different module, add serialization
impl fn serialize(p ~ Point) -> String { ... };
impl fn serialize(c ~ Circle) -> String { ... };
```

**3. Extensible Type Hierarchies**
```som
// Game entities that can interact
impl fn interact(p ~ Player, npc ~ NPC) { start_dialog(p, npc) };
impl fn interact(p ~ Player, item ~ Item) { pickup(p, item) };
impl fn interact(enemy ~ Enemy, p ~ Player) { attack(enemy, p) };
// Add new entity types and interactions without modifying existing code
```

---

## Syntax Design

### Declaring a Multimethod

```som
// Declare a multimethod signature (generic over types)
multimethod fn collide(a, b) -> ();
```

### Implementing Specific Cases

```som
// Provide implementations for specific type combinations
impl fn collide(a ~ Asteroid, b ~ Spaceship) {
    destroy(a);
    damage(b, 50);
};

impl fn collide(a ~ Spaceship, b ~ Asteroid) {
    damage(a, 50);
    destroy(b);
};

impl fn collide(a ~ Spaceship, b ~ Spaceship) {
    damage(a, 25);
    damage(b, 25);
};
```

### Calling Multimethods

```som
// Usage - looks like a regular function call
let main = fn() {
    let ship = Spaceship { ... };
    let rock = Asteroid { ... };

    collide(ship, rock);  // Compiler picks the (Spaceship, Asteroid) implementation
};
```

### Alternative Syntax (Optional)

If we want to avoid the `multimethod` keyword:

```som
// First implementation implicitly declares it as a multimethod
impl fn collide(a ~ Asteroid, b ~ Spaceship) {
    destroy(a);
    damage(b, 50);
};

// Additional implementations
impl fn collide(a ~ Spaceship, b ~ Asteroid) {
    damage(a, 50);
    destroy(b);
};
```

---

## Type System Changes

### New AST Nodes

```rust
// In ast/statement.rs

#[derive(Debug)]
pub struct MultimethodDeclaration<P: Phase> {
    pub span: Span,
    pub name: Identifier,
    pub parameters: Vec<Parameter<P>>,  // May be generic (no type specified)
    pub return_type: Option<Type>,
}

#[derive(Debug)]
pub struct MultimethodImplementation<P: Phase> {
    pub span: Span,
    pub name: Identifier,
    pub parameters: Vec<Parameter<P>>,  // Specific types
    pub return_type: Option<Type>,
    pub body: Box<Expression<P>>,
}
```

### Update Declaration Enum

```rust
pub enum Declaration<P: Phase> {
    Import(Import),
    ValueDefinition(ValueDefinition<P>),
    TypeDefinition(TypeDefinition),
    ExternDefinition(ExternDefinition),
    MultimethodDeclaration(MultimethodDeclaration<P>),      // NEW
    MultimethodImplementation(MultimethodImplementation<P>), // NEW
}
```

### Registry for Multimethods

In the type checker, we need a registry to track all implementations:

```rust
// In type_check/mod.rs

pub struct MultimethodRegistry {
    // Map from method name to all implementations
    methods: HashMap<String, Vec<MultimethodImpl>>,
}

struct MultimethodImpl {
    parameter_types: Vec<Type>,
    return_type: Type,
    span: Span,  // For error reporting
}
```

---

## Dispatch Algorithm

### Compile-Time Dispatch Resolution

Since Som is statically typed, we can resolve all dispatch at compile time.

**Algorithm:**

1. **Collect Candidates**: Find all implementations of the multimethod
2. **Filter by Arity**: Remove implementations with wrong number of parameters
3. **Filter by Type Compatibility**: Remove implementations where argument types don't match
4. **Select Most Specific**: Pick the implementation with the most specific parameter types
5. **Error if Ambiguous**: If multiple implementations are equally specific, report an error

### Specificity Rules

Type A is more specific than type B if:
- A is a subtype of B (when we add inheritance/traits)
- A is a concrete type and B is generic
- For now (without subtyping): A == B (exact match only)

### Example Resolution

```som
impl fn foo(a ~ i32, b ~ i32) { print("both i32") };
impl fn foo(a ~ i32, b ~ i64) { print("i32, i64") };
impl fn foo(a ~ i64, b ~ i32) { print("i64, i32") };

// Call site
foo(42, 100);
// Argument types: (i32, i32)
// Best match: foo(i32, i32) ✓
```

---

## Parser Modifications

### Grammar Updates

```
declaration ::=
    | import_declaration
    | value_definition
    | type_definition
    | extern_definition
    | multimethod_declaration    // NEW
    | multimethod_implementation // NEW

multimethod_declaration ::=
    "multimethod" "fn" identifier "(" parameter_list ")" ( "->" type )? ";"

multimethod_implementation ::=
    "impl" "fn" identifier "(" typed_parameter_list ")" ( "->" type )? block
```

### Parser Implementation

```rust
// In parser/statement.rs

fn parse_multimethod_declaration(parser: &mut Parser) -> Result<MultimethodDeclaration> {
    parser.expect(TokenKind::Multimethod, "multimethod")?;
    parser.expect(TokenKind::Fn, "fn")?;

    let name = Identifier::parse(parser)?;

    parser.expect(TokenKind::LeftParen, "(")?;
    let parameters = parse_parameter_list(parser)?;
    parser.expect(TokenKind::RightParen, ")")?;

    let return_type = if parser.peek(TokenKind::Arrow) {
        parser.next()?;
        Some(Type::parse(parser)?)
    } else {
        None
    };

    parser.expect(TokenKind::Semicolon, ";")?;

    Ok(MultimethodDeclaration {
        name,
        parameters,
        return_type,
    })
}

fn parse_multimethod_implementation(parser: &mut Parser) -> Result<MultimethodImplementation> {
    parser.expect(TokenKind::Impl, "impl")?;
    parser.expect(TokenKind::Fn, "fn")?;

    let name = Identifier::parse(parser)?;

    parser.expect(TokenKind::LeftParen, "(")?;
    let parameters = parse_typed_parameter_list(parser)?;
    parser.expect(TokenKind::RightParen, ")")?;

    let return_type = if parser.peek(TokenKind::Arrow) {
        parser.next()?;
        Some(Type::parse(parser)?)
    } else {
        None
    };

    let body = parse_block_or_expression(parser)?;

    Ok(MultimethodImplementation {
        name,
        parameters,
        return_type,
        body,
    })
}
```

---

## Type Checker Updates

### Phase 1: Declare Multimethods

In the `define` pass, collect all multimethod declarations and implementations:

```rust
// In type_check/module.rs

impl TypeChecker {
    fn define(&mut self, module: &Module<Untyped>) -> Result<()> {
        // ... existing code ...

        // Collect multimethod declarations
        for file in &module.files {
            for declaration in &file.declarations {
                if let Declaration::MultimethodDeclaration(decl) = declaration {
                    self.declare_multimethod(decl)?;
                }
            }
        }

        // Collect multimethod implementations
        for file in &module.files {
            for declaration in &file.declarations {
                if let Declaration::MultimethodImplementation(impl_) = declaration {
                    self.register_multimethod_impl(impl_)?;
                }
            }
        }

        Ok(())
    }

    fn declare_multimethod(&mut self, decl: &MultimethodDeclaration) -> Result<()> {
        let name = decl.name.to_string();

        // Create entry in registry if it doesn't exist
        self.multimethod_registry
            .methods
            .entry(name)
            .or_insert_with(Vec::new);

        Ok(())
    }

    fn register_multimethod_impl(&mut self, impl_: &MultimethodImplementation) -> Result<()> {
        let name = impl_.name.to_string();

        // Infer types for parameters
        let parameter_types: Vec<Type> = impl_
            .parameters
            .iter()
            .map(|p| p.ty.clone())
            .collect();

        let return_type = impl_.return_type.clone()
            .unwrap_or(Type::Unit);

        // Add to registry
        let implementations = self.multimethod_registry
            .methods
            .entry(name)
            .or_insert_with(Vec::new);

        // Check for duplicates
        for existing in implementations.iter() {
            if existing.parameter_types == parameter_types {
                return Err(TypeCheckError::DuplicateImplementation {
                    name: impl_.name.to_string(),
                    types: parameter_types,
                }.to_diagnostic());
            }
        }

        implementations.push(MultimethodImpl {
            parameter_types,
            return_type,
            span: impl_.span,
        });

        Ok(())
    }
}
```

### Phase 2: Resolve Call Sites

When type checking a function call, check if it's a multimethod:

```rust
// In type_check/expression.rs

impl TypeCheck for Call<Untyped> {
    fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Call<Typed>> {
        // Check if callee is a multimethod
        if let Expression::Primary(Primary {
            kind: PrimaryKind::Identifier(ref name),
            ..
        }) = *self.callee {
            if let Some(implementations) = ctx.multimethod_registry
                .methods
                .get(&name.to_string())
            {
                return self.type_check_multimethod_call(
                    name,
                    implementations,
                    ctx
                );
            }
        }

        // Otherwise, regular function call
        // ... existing code ...
    }

    fn type_check_multimethod_call(
        self,
        name: &Identifier,
        implementations: &[MultimethodImpl],
        ctx: &mut TypeCheckContext,
    ) -> Result<Call<Typed>> {
        // Type check all arguments
        let typed_args: Vec<Expression<Typed>> = self.arguments
            .into_iter()
            .map(|arg| arg.type_check(ctx))
            .collect::<Result<Vec<_>>>()?;

        let arg_types: Vec<Type> = typed_args
            .iter()
            .map(|arg| arg.ty().clone())
            .collect();

        // Find matching implementation
        let matching = implementations
            .iter()
            .filter(|impl_| {
                impl_.parameter_types.len() == arg_types.len()
                && impl_.parameter_types.iter()
                    .zip(&arg_types)
                    .all(|(param_ty, arg_ty)| param_ty == arg_ty)
            })
            .collect::<Vec<_>>();

        if matching.is_empty() {
            return Err(TypeCheckError::NoMatchingImplementation {
                name: name.to_string(),
                arg_types,
            }.to_diagnostic());
        }

        if matching.len() > 1 {
            return Err(TypeCheckError::AmbiguousCall {
                name: name.to_string(),
                candidates: matching.iter().map(|m| m.parameter_types.clone()).collect(),
            }.to_diagnostic());
        }

        let selected = matching[0];

        // Create typed call with the resolved implementation
        Ok(Call {
            span: self.span,
            callee: Box::new(/* reference to specific implementation */),
            arguments: typed_args,
            ty: selected.return_type.clone(),
        })
    }
}
```

---

## Code Generation Strategy

### Approach 1: Name Mangling (Simplest)

Each multimethod implementation gets a unique mangled name based on parameter types:

```som
impl fn collide(a ~ Asteroid, b ~ Spaceship) { ... };
// Generates: collide_Asteroid_Spaceship

impl fn collide(a ~ Spaceship, b ~ Asteroid) { ... };
// Generates: collide_Spaceship_Asteroid
```

At call sites, the type checker resolves which implementation to use, and we emit a direct call to the mangled name.

**Pros:**
- Simple to implement
- Zero runtime overhead
- Works with existing code generation

**Cons:**
- Function names can get long
- No runtime flexibility

### Approach 2: Dispatch Tables (Future)

For more complex scenarios (with subtyping):

```rust
// In emit/mod.rs

impl Emitter {
    fn emit_multimethod_dispatch(
        &mut self,
        name: &str,
        arg_types: &[Type],
        implementations: &[MultimethodImpl],
    ) -> Result<Value> {
        // Generate a dispatch table (like a vtable)
        // Index into it based on argument types
        // For now, this is overkill - use name mangling
    }
}
```

### Implementation in Emitter

```rust
// In emit/mod.rs

impl Emitter {
    fn compile(&mut self, modules: &[Module<Typed>]) -> Result<PathBuf> {
        // ... existing code ...

        // Compile all multimethod implementations
        for module in modules {
            for file in &module.files {
                for declaration in &file.declarations {
                    if let Declaration::MultimethodImplementation(impl_) = declaration {
                        self.compile_multimethod_impl(impl_)?;
                    }
                }
            }
        }

        // ... rest of compilation ...
    }

    fn compile_multimethod_impl(
        &mut self,
        impl_: &MultimethodImplementation<Typed>,
    ) -> Result<()> {
        // Generate mangled name
        let mangled_name = self.mangle_multimethod_name(
            &impl_.name.to_string(),
            &impl_.parameters.iter()
                .map(|p| p.ty.clone())
                .collect::<Vec<_>>(),
        );

        // Compile as a regular function with the mangled name
        // ... emit function body ...

        Ok(())
    }

    fn mangle_multimethod_name(&self, name: &str, param_types: &[Type]) -> String {
        let type_suffix = param_types
            .iter()
            .map(|ty| format!("{:?}", ty).replace(" ", "_"))
            .collect::<Vec<_>>()
            .join("_");

        format!("{}_{}", name, type_suffix)
    }
}
```

---

## Implementation Phases

### Phase 1: Basic Single-Argument Dispatch (Week 1-2)

**Goal:** Get the simplest case working

**Tasks:**
1. Add `multimethod` and `impl` keywords to lexer
2. Add AST nodes for multimethod declarations/implementations
3. Update parser to handle new syntax
4. Add multimethod registry to type checker
5. Implement dispatch resolution for single-argument functions
6. Implement name mangling in code generator
7. Write tests

**Example that should work:**
```som
multimethod fn greet(entity) -> ();

impl fn greet(p ~ Player) {
    print("Hello, player!");
};

impl fn greet(e ~ Enemy) {
    print("Hello, enemy!");
};
```

### Phase 2: Multi-Argument Dispatch (Week 3-4)

**Goal:** Support dispatch on multiple arguments

**Tasks:**
1. Extend dispatch resolution to handle N arguments
2. Test with binary operations (collide, interact, etc.)
3. Handle ambiguity errors
4. Optimize name mangling for multiple parameters

**Example that should work:**
```som
impl fn collide(a ~ Asteroid, b ~ Spaceship) { ... };
impl fn collide(a ~ Spaceship, b ~ Asteroid) { ... };
impl fn collide(a ~ Spaceship, b ~ Spaceship) { ... };
```

### Phase 3: Better Error Messages (Week 5)

**Goal:** Clear, helpful error messages

**Tasks:**
1. Show all candidate implementations when none match
2. Suggest closest matches based on type similarity
3. Show parameter type mismatches clearly

**Example error:**
```
error: no matching implementation for multimethod 'collide'
  --> example/game.som:42:5
   |
42 |     collide(ship, bullet);
   |     ^^^^^^^^^^^^^^^^^^^^^
   |
   | argument types: (Spaceship, Bullet)
   |
   | available implementations:
   |   - collide(Asteroid, Spaceship)    at example/game.som:10
   |   - collide(Spaceship, Asteroid)    at example/game.som:15
   |   - collide(Spaceship, Spaceship)   at example/game.som:20
   |
   = help: consider adding an implementation for (Spaceship, Bullet)
```

### Phase 4: Cross-Module Dispatch (Week 6)

**Goal:** Allow implementations in different modules

**Tasks:**
1. Track multimethod implementations across module boundaries
2. Handle visibility (private vs public implementations)
3. Prevent orphan-rule violations (optional)

### Phase 5: Optimization (Week 7-8)

**Goal:** Efficient code generation

**Tasks:**
1. Optimize name mangling (shorter names)
2. Inline small implementations
3. Benchmark against regular function calls

---

## Edge Cases & Error Handling

### 1. No Matching Implementation

```som
impl fn foo(x ~ i32) { ... };

foo("hello");  // ERROR: no implementation for foo(String)
```

**Error:**
```
error: no matching implementation for multimethod 'foo'
  --> example.som:3:1
   |
3 | foo("hello");
  | ^^^^^^^^^^^^ argument type: String
  |
  | available implementations:
  |   - foo(i32) at example.som:1
```

### 2. Ambiguous Call

```som
impl fn foo(x ~ i32, y ~ i32) { ... };
impl fn foo(x ~ i32, y ~ i64) { ... };

// If we had implicit conversions, this could be ambiguous
// For now, requires exact type match
```

### 3. Duplicate Implementation

```som
impl fn foo(x ~ i32) { print("first") };
impl fn foo(x ~ i32) { print("second") };  // ERROR
```

**Error:**
```
error: duplicate implementation for multimethod 'foo'
  --> example.som:2:1
   |
2 | impl fn foo(x ~ i32) { print("second") };
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
note: previous implementation here
  --> example.som:1:1
   |
1 | impl fn foo(x ~ i32) { print("first") };
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

### 4. Arity Mismatch

```som
impl fn foo(x ~ i32) { ... };
impl fn foo(x ~ i32, y ~ i32) { ... };

foo(1, 2, 3);  // ERROR: no implementation with 3 parameters
```

### 5. Return Type Mismatch

```som
multimethod fn compute(x) -> i32;

impl fn compute(x ~ i32) -> i64 {  // ERROR: return type doesn't match
    42
};
```

**Error:**
```
error: return type mismatch in multimethod implementation
  --> example.som:3:1
   |
3 | impl fn compute(x ~ i32) -> i64 {
  |                             ^^^ expected i32, found i64
  |
note: multimethod declared here
  --> example.som:1:1
   |
1 | multimethod fn compute(x) -> i32;
  |                              ^^^
```

---

## Examples

### Example 1: Game Entity Collisions

```som
// Define entity types
type Asteroid = { x ~ i32, y ~ i32, size ~ i32 };
type Spaceship = { x ~ i32, y ~ i32, health ~ i32 };
type Bullet = { x ~ i32, y ~ i32, damage ~ i32 };

// Declare the multimethod
multimethod fn collide(a, b) -> ();

// Implement for all combinations we care about
impl fn collide(a ~ Asteroid, b ~ Spaceship) {
    print("Asteroid hit spaceship!");
    b.health = b.health - 50;
};

impl fn collide(a ~ Spaceship, b ~ Asteroid) {
    print("Spaceship hit asteroid!");
    a.health = a.health - 50;
};

impl fn collide(a ~ Bullet, b ~ Asteroid) {
    print("Bullet destroyed asteroid!");
    // Remove asteroid from game
};

impl fn collide(a ~ Bullet, b ~ Spaceship) {
    print("Bullet hit spaceship!");
    b.health = b.health - a.damage;
};

// Main game loop
let main = fn() {
    let ship = Spaceship { x: 100, y: 100, health: 100 };
    let rock = Asteroid { x: 110, y: 105, size: 20 };
    let bullet = Bullet { x: 115, y: 108, damage: 25 };

    collide(ship, rock);    // Calls (Spaceship, Asteroid) version
    collide(bullet, rock);  // Calls (Bullet, Asteroid) version
};
```

### Example 2: Expression Serialization

```som
// Define expression types
type Literal = { value ~ i32 };
type Variable = { name ~ String };
type BinaryOp = { left ~ Expr, op ~ String, right ~ Expr };

type Expr = Literal | Variable | BinaryOp;

// Add serialization retroactively
multimethod fn serialize(expr) -> String;

impl fn serialize(e ~ Literal) -> String {
    to_string(e.value)
};

impl fn serialize(e ~ Variable) -> String {
    e.name
};

impl fn serialize(e ~ BinaryOp) -> String {
    let left = serialize(e.left);
    let right = serialize(e.right);
    concat(left, " ", e.op, " ", right)
};

let main = fn() {
    let expr = BinaryOp {
        left: Variable { name: "x" },
        op: "+",
        right: Literal { value: 42 }
    };

    let result = serialize(expr);
    print(result);  // Prints: "x + 42"
};
```

### Example 3: Type-Based Formatting

```som
multimethod fn format(value) -> String;

impl fn format(x ~ i32) -> String {
    to_string(x)
};

impl fn format(x ~ String) -> String {
    concat("\"", x, "\"")
};

impl fn format(x ~ bool) -> String {
    if x { "true" } else { "false" }
};

let main = fn() {
    print(format(42));        // "42"
    print(format("hello"));   // "\"hello\""
    print(format(true));      // "true"
};
```

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dispatch() {
        let code = r#"
            multimethod fn greet(x) -> String;

            impl fn greet(x ~ i32) -> String {
                "number"
            };

            impl fn greet(x ~ String) -> String {
                "string"
            };

            let main = fn() {
                greet(42)
            };
        "#;

        let result = compile_and_run(code);
        assert_eq!(result, "number");
    }

    #[test]
    fn test_multi_arg_dispatch() {
        let code = r#"
            impl fn add(a ~ i32, b ~ i32) -> i32 { a + b };
            impl fn add(a ~ String, b ~ String) -> String { concat(a, b) };

            let main = fn() {
                add(1, 2)
            };
        "#;

        let result = compile_and_run(code);
        assert_eq!(result, 3);
    }

    #[test]
    fn test_no_matching_impl() {
        let code = r#"
            impl fn foo(x ~ i32) { print("i32") };

            let main = fn() {
                foo("hello")
            };
        "#;

        let result = compile(code);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no matching implementation"));
    }
}
```

### Integration Tests

Create test files in `tests/multimethod/`:
- `basic.som` - Simple single-arg dispatch
- `multi_arg.som` - Multiple argument dispatch
- `cross_module.som` - Dispatch across modules
- `errors.som` - Error cases

---

## Future Extensions

### 1. Predicate Dispatch

Dispatch based on predicates, not just types:

```som
impl fn factorial(n ~ i32) where n == 0 -> i32 {
    1
};

impl fn factorial(n ~ i32) where n > 0 -> i32 {
    n * factorial(n - 1)
};
```

### 2. Default Implementations

```som
multimethod fn process(x) -> ();

impl fn process(x ~ *) {  // Catch-all default
    print("Unknown type");
};

impl fn process(x ~ i32) {  // More specific
    print("Integer");
};
```

### 3. Method Combinations (CLOS-style)

```som
impl fn save:before(doc ~ Document) {
    validate(doc);
    create_backup(doc);
};

impl fn save(doc ~ Document) {
    write_to_disk(doc);
};

impl fn save:after(doc ~ Document) {
    update_timestamp(doc);
};
```

---

## Conclusion

Multiple dispatch would be a powerful addition to Som, enabling:
- ✅ **Symmetric operations** between types
- ✅ **Retroactive extension** of functionality
- ✅ **Cleaner code** for type-based behavior
- ✅ **Compile-time safety** with static dispatch

The implementation is tractable, with clear phases and no need for runtime overhead. The syntax fits naturally into Som's existing design.

Next steps: Begin Phase 1 implementation.

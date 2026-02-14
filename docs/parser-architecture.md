# Parser Architecture: From Theory to Practice

*A comprehensive guide to designing and implementing parsers, exploring the theory, history, and techniques that transform text into meaning.*

---

## Table of Contents

1. [Introduction: What Is Parsing?](#introduction-what-is-parsing)
2. [A Brief History of Parsing](#a-brief-history-of-parsing)
3. [Grammar Theory: The Foundation](#grammar-theory-the-foundation)
4. [Recursive Descent: The Workhorse](#recursive-descent-the-workhorse)
5. [Pratt Parsing: Elegant Expressions](#pratt-parsing-elegant-expressions)
6. [Error Recovery: When Things Go Wrong](#error-recovery-when-things-go-wrong)
7. [Industry Patterns: Learning from the Giants](#industry-patterns-learning-from-the-giants)
8. [Rust Patterns for Parsers](#rust-patterns-for-parsers)
9. [Architecture Design: Separating Concerns](#architecture-design-separating-concerns)
10. [Implementation Walkthrough](#implementation-walkthrough)
11. [Testing Strategy](#testing-strategy)
12. [Further Reading](#further-reading)

---

## Introduction: What Is Parsing?

Parsing is the art of imposing structure on chaos. A source file is just a sequence of bytes. Parsing transforms it into a tree that captures the programmer's intent—what they meant, not just what they typed.

```
let x = 1 + 2 * 3;
```

To a lexer, this is a sequence of tokens:
```
LET, IDENT("x"), EQUALS, INT(1), PLUS, INT(2), STAR, INT(3), SEMICOLON
```

To a parser, this is a tree:
```
Let {
    name: "x",
    value: Binary {
        op: Add,
        left: Int(1),
        right: Binary {
            op: Mul,
            left: Int(2),
            right: Int(3)
        }
    }
}
```

Notice how `2 * 3` is grouped together, not `1 + 2`. The parser understands precedence. The parser understands structure. The parser transforms a flat sequence into a hierarchy of meaning.

### Why Parsing Is Hard

Parsing seems simple: match patterns, build trees. But real languages present challenges:

**Ambiguity**: Does `if a if b else c` mean `if a (if b else c)` or `(if a if b) else c`?

**Lookahead**: When you see `foo`, is it a variable, a function call `foo()`, or a struct literal `foo { x: 1 }`? You can't know until you see what comes next.

**Error recovery**: When the programmer makes a mistake, what do you do? Stop entirely? Guess what they meant? The parser must keep going to report multiple errors.

**Precedence and associativity**: Is `1 - 2 - 3` equal to `(1 - 2) - 3` or `1 - (2 - 3)`? Operators have rules.

**Context sensitivity**: In C, `(x)(y)` might be a cast or a function call, depending on whether `x` is a type name or a variable.

The history of parsing is the history of solving these problems elegantly.

### The Parser's Contract

A good parser provides:

1. **Correctness**: Valid programs produce correct ASTs
2. **Good errors**: Invalid programs produce helpful diagnostics
3. **Completeness**: The parser handles the full language
4. **Performance**: Parsing is fast (usually linear time)
5. **Maintainability**: Adding new syntax is straightforward

Achieving all five simultaneously is the challenge.

---

## A Brief History of Parsing

### The Chomsky Hierarchy (1956-1959)

Noam Chomsky, a linguist studying natural language, classified grammars into four types:

**Type 3 - Regular grammars**: Can be recognized by finite automata. Good for tokens (identifiers, numbers). Not powerful enough for nested structures.

**Type 2 - Context-free grammars (CFG)**: Can be recognized by pushdown automata. The sweet spot for programming languages. Can handle nesting, recursion, most syntax.

**Type 1 - Context-sensitive grammars**: More powerful but harder to parse. Rarely needed for programming languages.

**Type 0 - Unrestricted grammars**: Turing-complete. Not useful for parsing.

Programming languages mostly use context-free grammars (CFG), sometimes with context-sensitive extensions handled outside the parser (like type checking).

### The Parsing Wars (1960s-1970s)

Early compiler writers faced a choice: how to parse context-free grammars?

**Top-down parsing**: Start from the root (the start symbol) and try to derive the input. Natural, easy to implement by hand, but limited in what grammars it can handle.

**Bottom-up parsing**: Start from the input and try to reduce it to the start symbol. More powerful, handles more grammars, but harder to implement by hand.

Two camps emerged:

**LL (Left-to-right, Leftmost derivation)**: Top-down. Reads input left-to-right, produces a leftmost derivation. LL(k) can look ahead k tokens. LL(1) is the most common.

**LR (Left-to-right, Rightmost derivation)**: Bottom-up. More powerful than LL—can handle more grammars—but requires tables that are hard to construct by hand.

### YACC and the Parser Generator Era (1970s-1990s)

Stephen Johnson created YACC (Yet Another Compiler-Compiler) at Bell Labs in 1975. YACC takes a grammar and generates a parser automatically.

```yacc
expr:   expr '+' term   { $$ = make_add($1, $3); }
    |   term
    ;

term:   term '*' factor { $$ = make_mul($1, $3); }
    |   factor
    ;
```

Parser generators like YACC (and later Bison, ANTLR, etc.) dominated for decades. The appeal was clear: describe your grammar, get a parser. But problems emerged:

- **Poor error messages**: Generated parsers gave cryptic errors like "syntax error at token FOO"
- **Grammar debugging**: Understanding why a grammar was ambiguous required understanding the algorithm
- **Inflexibility**: Customizing behavior meant fighting the generator
- **Build complexity**: Another tool in the build pipeline, another language to learn

### The Handwritten Parser Renaissance (2000s-present)

A surprising trend emerged: major compilers abandoned parser generators for handwritten parsers.

**GCC** (GNU C Compiler): Switched from Bison to handwritten recursive descent in 2004.

**Clang** (LLVM C/C++): Always handwritten, specifically to provide better error messages.

**Go**: Handwritten from the start. Rob Pike: "We generate nothing in Go."

**Rust**: Handwritten recursive descent.

**Swift**: Handwritten, with sophisticated error recovery.

**TypeScript**: Handwritten, evolved from JavaScript's parser.

Why? Because:

1. **Error messages**: Handwritten parsers can provide context-sensitive, helpful diagnostics
2. **Error recovery**: You can implement sophisticated recovery strategies
3. **Performance**: No table lookup overhead, better cache behavior
4. **Flexibility**: Handle context-sensitive cases naturally
5. **Understandability**: The code *is* the grammar, no separate specification

The consensus today: for production compilers, handwritten parsers are preferred. Parser generators still have their place for quick prototypes, DSLs, and languages where the grammar is truly the focus.

### Pratt Parsing: The Forgotten Gem (1973)

Vaughan Pratt published "Top Down Operator Precedence" in 1973. It described a beautiful technique for parsing expressions with precedence—simpler than recursive descent, more flexible than precedence climbing.

For decades, it was largely forgotten. Then in 2007, Douglas Crockford wrote about using it in JavaScript's JSLint, and the technique experienced a renaissance. Today, Pratt parsing (also called "top-down operator precedence" or "precedence climbing") is the standard approach for expression parsing in handwritten compilers.

---

## Grammar Theory: The Foundation

Before writing a parser, you need to understand what you're parsing. Grammars provide the formal specification.

### Context-Free Grammars

A context-free grammar consists of:

- **Terminals**: The tokens from the lexer (keywords, operators, literals)
- **Non-terminals**: Abstract syntactic categories (expression, statement, type)
- **Productions**: Rules showing how non-terminals expand
- **Start symbol**: Where parsing begins

Example grammar for simple arithmetic:

```
expr   → term (('+' | '-') term)*
term   → factor (('*' | '/') factor)*
factor → NUMBER | '(' expr ')'
```

This is in EBNF (Extended Backus-Naur Form), where `*` means "zero or more" and `|` means "or".

### Derivations and Parse Trees

Given input `1 + 2 * 3`, here's a leftmost derivation:

```
expr
→ term (('+' | '-') term)*
→ factor (('*' | '/') factor)* (('+' | '-') term)*
→ NUMBER (('*' | '/') factor)* (('+' | '-') term)*
→ 1 (('+' | '-') term)*
→ 1 '+' term
→ 1 '+' factor (('*' | '/') factor)*
→ 1 '+' NUMBER (('*' | '/') factor)*
→ 1 '+' 2 (('*' | '/') factor)*
→ 1 '+' 2 '*' factor
→ 1 '+' 2 '*' NUMBER
→ 1 + 2 * 3
```

The parse tree captures this structure:

```
        expr
       /    \
    term  '+' term
      |        |
   factor   factor '*' factor
      |        |         |
      1        2         3
```

Notice how the grammar encodes precedence: `*` binds tighter because it's deeper in the grammar hierarchy.

### Ambiguity

A grammar is **ambiguous** if some input has multiple parse trees.

Classic example—the dangling else:

```
stmt → 'if' expr 'then' stmt
     | 'if' expr 'then' stmt 'else' stmt
     | other
```

Input: `if a then if b then x else y`

Two valid parses:
```
if a then (if b then x else y)   // else binds to inner if
if a then (if b then x) else y   // else binds to outer if
```

Ambiguity must be resolved. Options:

1. **Rewrite the grammar** to be unambiguous
2. **Add disambiguation rules** (else binds to nearest if)
3. **Use precedence declarations** in parser generators
4. **Handle in the parser** with explicit code

### Left Recursion

Left-recursive rules cause problems for top-down parsers:

```
expr → expr '+' term | term
```

If you try to parse `expr` by first parsing `expr`, you recurse forever!

**Solution 1: Left factoring**

Transform to right recursion:
```
expr  → term expr'
expr' → '+' term expr' | ε
```

**Solution 2: Use iteration**

```
expr → term ('+' term)*
```

This is what most handwritten parsers do—use loops instead of recursion.

**Solution 3: Pratt parsing**

Pratt parsing handles left recursion naturally through its loop structure. More on this later.

### First and Follow Sets

For LL parsing, you need to know which terminals can begin a non-terminal (the First set) and which can follow it (the Follow set).

```
First(expr)   = { NUMBER, '(' }
First(term)   = { NUMBER, '(' }
First(factor) = { NUMBER, '(' }

Follow(expr)   = { ')', EOF }
Follow(term)   = { '+', '-', ')', EOF }
Follow(factor) = { '*', '/', '+', '-', ')', EOF }
```

These sets guide parsing decisions: "I see a `(`, so I must be starting a factor."

For handwritten parsers, you usually encode this knowledge implicitly in the code rather than computing it formally.

---

## Recursive Descent: The Workhorse

Recursive descent is the most common technique for handwritten parsers. The idea is simple: one function per non-terminal, with the function structure mirroring the grammar.

### Basic Structure

For this grammar:
```
program → decl*
decl    → 'fn' IDENT '(' params ')' block
        | 'let' IDENT '=' expr ';'
```

The parser:

```rust
fn parse_program(&mut self) {
    while !self.at_eof() {
        self.parse_decl();
    }
}

fn parse_decl(&mut self) {
    if self.at(Token::Fn) {
        self.parse_function();
    } else if self.at(Token::Let) {
        self.parse_let();
    } else {
        self.error("expected declaration");
    }
}

fn parse_function(&mut self) {
    self.expect(Token::Fn);
    let name = self.parse_ident();
    self.expect(Token::OpenParen);
    let params = self.parse_params();
    self.expect(Token::CloseParen);
    let body = self.parse_block();
    // Build AST node...
}
```

### The Parsing Toolkit

Every recursive descent parser needs these primitives:

```rust
impl Parser {
    /// Look at current token without consuming
    fn peek(&self) -> TokenKind {
        self.tokens[self.pos].kind
    }

    /// Check if current token matches
    fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    /// Check if at end of file
    fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    /// Advance to next token
    fn advance(&mut self) {
        self.pos += 1;
        self.skip_trivia();  // Skip whitespace, comments
    }

    /// Consume token if it matches, return success
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Require token or error
    fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            let span = self.current_span();
            self.advance();
            Some(span)
        } else {
            self.error_expected(kind);
            None
        }
    }
}
```

### Handling Optional Elements

Grammar: `let_stmt → 'let' 'mut'? IDENT (':' type)? '=' expr ';'`

```rust
fn parse_let(&mut self) -> Option<Stmt> {
    self.expect(Token::Let)?;

    // Optional 'mut'
    let mutable = self.eat(Token::Mut);

    let name = self.parse_ident()?;

    // Optional type annotation
    let ty = if self.eat(Token::Colon) {
        Some(self.parse_type()?)
    } else {
        None
    };

    self.expect(Token::Equals)?;
    let value = self.parse_expr()?;
    self.expect(Token::Semicolon)?;

    Some(Stmt::Let { name, mutable, ty, value })
}
```

### Handling Repetition

Grammar: `params → (param (',' param)*)?`

```rust
fn parse_params(&mut self) -> Vec<Param> {
    let mut params = Vec::new();

    // Check for empty parameter list
    if self.at(Token::CloseParen) {
        return params;
    }

    // Parse first parameter
    params.push(self.parse_param());

    // Parse remaining parameters
    while self.eat(Token::Comma) {
        // Allow trailing comma
        if self.at(Token::CloseParen) {
            break;
        }
        params.push(self.parse_param());
    }

    params
}
```

### The Problem with Left Recursion

Consider:
```
expr → expr '+' term | term
```

A naive implementation:

```rust
fn parse_expr(&mut self) -> Expr {
    let left = self.parse_expr();  // INFINITE RECURSION!
    if self.eat(Token::Plus) {
        let right = self.parse_term();
        Expr::Add(left, right)
    } else {
        left
    }
}
```

This loops forever. Solution: restructure as iteration:

```rust
fn parse_expr(&mut self) -> Expr {
    let mut left = self.parse_term();
    while self.at(Token::Plus) || self.at(Token::Minus) {
        let op = self.advance_op();
        let right = self.parse_term();
        left = Expr::Binary { op, left, right };
    }
    left
}
```

But what about precedence? If we have `+`, `-`, `*`, `/` all with different precedences, do we need a separate function for each level?

```rust
fn parse_expr(&mut self) -> Expr {
    self.parse_additive()
}

fn parse_additive(&mut self) -> Expr {
    let mut left = self.parse_multiplicative();
    while self.at(Token::Plus) || self.at(Token::Minus) {
        let op = self.advance_op();
        let right = self.parse_multiplicative();
        left = Expr::Binary { op, left, right };
    }
    left
}

fn parse_multiplicative(&mut self) -> Expr {
    let mut left = self.parse_unary();
    while self.at(Token::Star) || self.at(Token::Slash) {
        let op = self.advance_op();
        let right = self.parse_unary();
        left = Expr::Binary { op, left, right };
    }
    left
}

fn parse_unary(&mut self) -> Expr {
    // ... and so on
}
```

This works but gets tedious with many precedence levels. Enter Pratt parsing.

---

## Pratt Parsing: Elegant Expressions

Pratt parsing (also called "top-down operator precedence") is a technique for parsing expressions that elegantly handles precedence and associativity. It's the secret weapon of production parsers.

### The Core Insight

Every operator has two "binding powers":

- **Left binding power (lbp)**: How tightly it binds to the expression on its left
- **Right binding power (rbp)**: How tightly it binds to the expression on its right

For left-associative operators: `lbp < rbp`
For right-associative operators: `lbp > rbp`

Example binding powers:

```
Operator    lbp    rbp    Associativity
--------    ---    ---    -------------
||           1      2     left
&&           3      4     left
== !=        5      6     left
< > <= >=    7      8     left
+ -          9     10     left
* /         11     12     left
! - &       --     14     prefix (right)
() [] .     16     --     postfix (left)
```

Higher numbers mean tighter binding. `*` at 11-12 binds tighter than `+` at 9-10.

### The Algorithm

```rust
fn parse_expr(&mut self) -> Expr {
    self.parse_expr_bp(0)  // Start with minimum binding power
}

fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
    // Parse prefix/atom (the left-hand side)
    let mut lhs = self.parse_prefix();

    loop {
        // Try to parse an infix/postfix operator
        let Some((op, lbp, rbp)) = self.peek_infix() else {
            break;
        };

        // Stop if this operator binds less tightly than our minimum
        if lbp < min_bp {
            break;
        }

        self.advance();  // Consume the operator

        // Recursively parse the right-hand side
        let rhs = self.parse_expr_bp(rbp);

        // Build the AST node
        lhs = Expr::Binary { op, lhs, rhs };
    }

    lhs
}
```

### A Concrete Example

Let's trace through parsing `1 + 2 * 3`:

```
parse_expr_bp(0):
  parse_prefix() -> 1
  lhs = 1

  peek_infix() -> (+, lbp=9, rbp=10)
  9 >= 0? yes, continue
  advance() [consume +]

  parse_expr_bp(10):
    parse_prefix() -> 2
    lhs = 2

    peek_infix() -> (*, lbp=11, rbp=12)
    11 >= 10? yes, continue
    advance() [consume *]

    parse_expr_bp(12):
      parse_prefix() -> 3
      lhs = 3

      peek_infix() -> None (semicolon or EOF)
      break
    return 3

    rhs = 3
    lhs = 2 * 3

    peek_infix() -> None
    break
  return 2 * 3

  rhs = 2 * 3
  lhs = 1 + (2 * 3)

  peek_infix() -> None
  break
return 1 + (2 * 3)
```

The multiplication is parsed first because its `lbp` (11) exceeds our `min_bp` (10).

### Handling Prefix Operators

Prefix operators (like unary `-`, `!`, `&`, `*`) are handled in `parse_prefix`:

```rust
fn parse_prefix(&mut self) -> Expr {
    match self.peek() {
        Token::Minus => {
            self.advance();
            let operand = self.parse_expr_bp(14);  // High precedence
            Expr::Negate(operand)
        }
        Token::Bang => {
            self.advance();
            let operand = self.parse_expr_bp(14);
            Expr::Not(operand)
        }
        Token::Ampersand => {
            self.advance();
            let mutable = self.eat(Token::Mut);
            let operand = self.parse_expr_bp(14);
            Expr::Borrow { mutable, operand }
        }
        _ => self.parse_atom()
    }
}
```

### Handling Postfix Operators

Postfix operators (function calls, indexing, field access) are handled in the main loop:

```rust
fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
    let mut lhs = self.parse_prefix();

    loop {
        // Check for postfix operators first
        match self.peek() {
            Token::OpenParen => {
                if 16 < min_bp { break; }  // Call has high precedence
                self.advance();
                let args = self.parse_args();
                self.expect(Token::CloseParen);
                lhs = Expr::Call { func: lhs, args };
                continue;
            }
            Token::Dot => {
                if 16 < min_bp { break; }
                self.advance();
                let field = self.parse_ident();
                lhs = Expr::FieldAccess { object: lhs, field };
                continue;
            }
            Token::OpenBracket => {
                if 16 < min_bp { break; }
                self.advance();
                let index = self.parse_expr();
                self.expect(Token::CloseBracket);
                lhs = Expr::Index { array: lhs, index };
                continue;
            }
            _ => {}
        }

        // Then check for infix operators
        let Some((op, lbp, rbp)) = self.peek_infix() else { break };
        if lbp < min_bp { break; }
        // ... rest of infix handling
    }

    lhs
}
```

### Handling Mixfix Operators

Some operators don't fit the prefix/infix/postfix model cleanly. Consider the ternary conditional:

```
a if condition else b     // Python-style
condition ? a : b          // C-style
```

For `a if condition else b`, `if` acts like an infix operator:

```rust
// In the main loop, after checking for postfix:
Token::If => {
    let if_bp = 2;  // Very low precedence
    if if_bp < min_bp { break; }
    self.advance();  // consume 'if'

    let condition = self.parse_expr();
    self.expect(Token::Else);
    let falsy = self.parse_expr_bp(if_bp);  // Right associative

    lhs = Expr::Conditional { truthy: lhs, condition, falsy };
    continue;
}
```

### Associativity Revisited

For left associativity (`1 - 2 - 3` = `(1 - 2) - 3`):
- lbp < rbp
- When we see the second `-`, its lbp doesn't exceed our min_bp (which was set to the first `-`'s rbp)
- So we stop and let the outer call handle it

For right associativity (`a = b = c` = `a = (b = c)`):
- lbp > rbp
- When we see the second `=`, its lbp exceeds our min_bp
- So we continue parsing and build the inner `=` first

The binding power difference encodes associativity without any special code!

### A Complete Binding Power Table

```rust
pub struct Grammar;

impl Grammar {
    pub fn infix_binding_power(kind: TokenKind) -> Option<(BinOp, u8, u8)> {
        match kind {
            // Assignment (right associative)
            TokenKind::Equals => Some((BinOp::Assign, 2, 1)),

            // Logical or (left associative)
            TokenKind::PipePipe => Some((BinOp::Or, 3, 4)),

            // Logical and (left associative)
            TokenKind::AmpAmp => Some((BinOp::And, 5, 6)),

            // Equality (left associative)
            TokenKind::EqEq => Some((BinOp::Eq, 7, 8)),
            TokenKind::NotEq => Some((BinOp::NotEq, 7, 8)),

            // Comparison (non-associative)
            TokenKind::Lt => Some((BinOp::Lt, 9, 9)),
            TokenKind::Gt => Some((BinOp::Gt, 9, 9)),
            TokenKind::LtEq => Some((BinOp::LtEq, 9, 9)),
            TokenKind::GtEq => Some((BinOp::GtEq, 9, 9)),

            // Additive (left associative)
            TokenKind::Plus => Some((BinOp::Add, 11, 12)),
            TokenKind::Minus => Some((BinOp::Sub, 11, 12)),

            // Multiplicative (left associative)
            TokenKind::Star => Some((BinOp::Mul, 13, 14)),
            TokenKind::Slash => Some((BinOp::Div, 13, 14)),
            TokenKind::Percent => Some((BinOp::Mod, 13, 14)),

            _ => None,
        }
    }

    pub fn prefix_binding_power(kind: TokenKind) -> Option<u8> {
        match kind {
            TokenKind::Minus => Some(15),
            TokenKind::Bang => Some(15),
            TokenKind::Ampersand => Some(15),
            TokenKind::Star => Some(15),
            _ => None,
        }
    }

    pub fn postfix_binding_power(kind: TokenKind) -> Option<u8> {
        match kind {
            TokenKind::OpenParen => Some(17),   // Call
            TokenKind::OpenBracket => Some(17), // Index
            TokenKind::Dot => Some(17),         // Field access
            _ => None,
        }
    }
}
```

### Why Pratt Parsing Works

Pratt parsing elegantly solves several problems:

1. **Precedence**: Higher binding powers mean tighter binding
2. **Associativity**: The lbp/rbp difference encodes direction
3. **Left recursion**: The loop handles it naturally
4. **Extensibility**: Adding an operator means adding one entry
5. **Understandability**: The table *is* the precedence spec

This is why it's the preferred technique for expression parsing in production compilers.

---

## Error Recovery: When Things Go Wrong

Parsing is easy when input is correct. The real challenge is handling incorrect input gracefully.

### The Goals of Error Recovery

1. **Report the actual error**: Tell the user what went wrong
2. **Report the location**: Point to where it went wrong
3. **Avoid cascading errors**: Don't report 50 errors from one typo
4. **Keep parsing**: Find more errors in one pass
5. **Don't crash**: Invalid input shouldn't cause panics

### The Naive Approach: Stop on First Error

```rust
fn expect(&mut self, kind: TokenKind) -> Token {
    if self.at(kind) {
        self.advance()
    } else {
        panic!("expected {:?}, found {:?}", kind, self.peek());
    }
}
```

Problems:
- User has to fix one error at a time
- Terrible developer experience
- Makes the compiler seem fragile

### Panic Mode Recovery

The classic approach: when an error occurs, skip tokens until you find a "synchronization point" where parsing can resume.

```rust
fn synchronize(&mut self) {
    while !self.at_eof() {
        // Stop at statement/declaration boundaries
        match self.peek() {
            TokenKind::Fn |
            TokenKind::Let |
            TokenKind::If |
            TokenKind::While |
            TokenKind::Return |
            TokenKind::CloseBrace => return,

            // Also stop after semicolons
            TokenKind::Semicolon => {
                self.advance();
                return;
            }

            _ => self.advance(),
        }
    }
}

fn expect_recover(&mut self, kind: TokenKind) -> bool {
    if self.at(kind) {
        self.advance();
        true
    } else {
        self.error(format!("expected {}", kind));
        self.synchronize();
        false
    }
}
```

This gets you back on track but loses all context about what you were parsing.

### Multi-Level Recovery

Different constructs need different recovery strategies:

```rust
enum RecoveryLevel {
    /// Sync at expression boundaries: ), ], }, ;, ,
    Expression,
    /// Sync at statement boundaries: ;, }, let, if, while
    Statement,
    /// Sync at declaration boundaries: fn, struct, impl, }
    Declaration,
}

fn recover(&mut self, level: RecoveryLevel) {
    let sync_tokens = match level {
        RecoveryLevel::Expression => &[
            TokenKind::CloseParen,
            TokenKind::CloseBracket,
            TokenKind::CloseBrace,
            TokenKind::Semicolon,
            TokenKind::Comma,
        ],
        RecoveryLevel::Statement => &[
            TokenKind::Semicolon,
            TokenKind::CloseBrace,
            TokenKind::Let,
            TokenKind::If,
            TokenKind::While,
            TokenKind::Return,
        ],
        RecoveryLevel::Declaration => &[
            TokenKind::Fn,
            TokenKind::Struct,
            TokenKind::Impl,
            TokenKind::CloseBrace,
        ],
    };

    while !self.at_eof() {
        if sync_tokens.contains(&self.peek()) {
            return;
        }

        // Handle balanced delimiters
        if self.at(TokenKind::OpenBrace) {
            self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
            continue;
        }
        if self.at(TokenKind::OpenParen) {
            self.skip_balanced(TokenKind::OpenParen, TokenKind::CloseParen);
            continue;
        }

        self.advance();
    }
}
```

### Avoiding Cascading Errors

One error often triggers many more:

```
fn foo() {
    let x =       // Missing expression
    let y = 1;    // Looks like an error because we're confused
    let z = 2;    // Same here
}
```

Solution: track whether we're "in recovery mode" and suppress errors until we're back on track.

```rust
struct Parser {
    // ...
    in_recovery: bool,
    last_error_pos: usize,
}

fn error(&mut self, msg: &str) {
    // Don't report if we're still recovering from a recent error
    if self.in_recovery && self.pos - self.last_error_pos < 5 {
        return;
    }

    self.errors.push(ParseError { msg, span: self.current_span() });
    self.in_recovery = true;
    self.last_error_pos = self.pos;
}

fn parse_success(&mut self) {
    // Successfully parsed something - we're back on track
    self.in_recovery = false;
}
```

### Expected Token Tracking

Build up a list of what tokens would be valid:

```rust
struct Parser {
    expected: Vec<TokenKind>,
}

fn at(&mut self, kind: TokenKind) -> bool {
    self.expected.push(kind);  // Record that this was a possibility
    self.peek() == kind
}

fn error_unexpected(&mut self) {
    let msg = if self.expected.is_empty() {
        format!("unexpected token {:?}", self.peek())
    } else if self.expected.len() == 1 {
        format!("expected {:?}, found {:?}", self.expected[0], self.peek())
    } else {
        format!(
            "expected one of {:?}, found {:?}",
            self.expected,
            self.peek()
        )
    };
    self.error(&msg);
    self.expected.clear();
}
```

### Error Productions

Sometimes the grammar itself can specify error handling:

```rust
fn parse_function(&mut self) {
    self.expect(TokenKind::Fn)?;

    let name = match self.parse_ident() {
        Some(name) => name,
        None => {
            self.error("expected function name");
            Ident::error()  // Placeholder so we can continue
        }
    };

    // Even if name failed, try to parse the rest
    self.expect(TokenKind::OpenParen);
    let params = self.parse_params();
    self.expect(TokenKind::CloseParen);

    // ...
}
```

### Hole Expressions

When an expression is missing, insert a "hole" that type checking will catch:

```rust
fn parse_expr(&mut self) -> Id<Expr> {
    if let Some(expr) = self.try_parse_expr() {
        expr
    } else {
        self.error("expected expression");
        // Return a placeholder
        self.ast.alloc_expr(Expr::Hole)
    }
}
```

The `Hole` propagates through type checking, which can provide additional diagnostics like "expected i32, found _".

### Real-World Error Recovery: rustc

Rust's compiler has sophisticated error recovery:

```rust
// When parsing `fn foo(x: i32` with missing `)`:
// rustc notices the unbalanced parens, suggests adding `)`

// When parsing `let x = 1 + ;`:
// rustc notices the missing operand, creates a hole, suggests what was expected

// When parsing `fn foo() { let x = }`:
// rustc recovers at the `}`, reports the missing expression
```

This requires significant engineering effort, but the result is a compiler that feels helpful rather than hostile.

---

## Industry Patterns: Learning from the Giants

How do production compilers structure their parsers? Let's examine several approaches.

### rustc: The Rust Compiler

**Architecture**: Hand-written recursive descent with sophisticated error recovery.

**Key patterns**:

1. **Restriction flags**: Context-sensitive parsing
   ```rust
   struct Restrictions {
       stmt_expr: bool,      // In statement position?
       no_struct_literal: bool,  // Struct literals forbidden?
   }
   ```

2. **Expected token tracking**: For error messages
   ```rust
   fn expect(&mut self, t: &TokenKind) -> PResult<bool> {
       if self.token == *t {
           self.bump();
           Ok(true)
       } else {
           self.unexpected()  // Uses expected set
       }
   }
   ```

3. **Snapshot/restore**: For speculative parsing
   ```rust
   let snapshot = self.create_snapshot_for_diagnostic();
   if self.try_parse_something().is_err() {
       self.restore_snapshot(snapshot);
   }
   ```

4. **Recovery functions**: Specific recovery for specific contexts
   ```rust
   fn recover_closing_paren(&mut self) {
       // Try to find matching )
   }
   ```

### Clang: The C/C++ Compiler

**Architecture**: Hand-written recursive descent with tentative parsing.

**Key patterns**:

1. **Tentative parsing**: Try multiple interpretations
   ```cpp
   // Is (x)(y) a cast or function call?
   // Tentatively parse as cast, backtrack if wrong
   TentativeParsingAction TPA(*this);
   if (ParseCastExpression()) {
       TPA.Commit();
   } else {
       TPA.Revert();
       // Try function call
   }
   ```

2. **Action callbacks**: Separate parsing from AST building
   ```cpp
   class Parser {
       Sema &Actions;  // Semantic actions
   };
   ```

3. **Extensive caching**: For performance in header-heavy C++

### Go: The Go Compiler

**Architecture**: Hand-written, deliberately simple.

**Key patterns**:

1. **Single-pass**: No backtracking, simple lookahead
   ```go
   func (p *parser) expr() Expr {
       return p.binaryExpr(0)
   }
   ```

2. **Error lists**: Collect all errors
   ```go
   type parser struct {
       errors []error
   }
   ```

3. **Semicolon insertion**: Done in the lexer, simplifies parser
   ```go
   // Lexer inserts `;` after certain tokens at line end
   ```

### Swift: The Swift Compiler

**Architecture**: Hand-written with sophisticated recovery.

**Key patterns**:

1. **Lookahead helpers**: For disambiguation
   ```cpp
   bool canParseAsAttribute();
   bool canParseAsGenericArgument();
   ```

2. **Recovery helpers**: Contextual recovery
   ```cpp
   ParserStatus parseMatchingToken(tok K, SourceLoc &TokLoc);
   ```

3. **Separation of concerns**: Parse, validate, diagnose are distinct

### Common Themes

Across all these compilers:

1. **Hand-written**: Not generated from grammars
2. **Recursive descent**: With Pratt for expressions
3. **Sophisticated recovery**: Multiple strategies
4. **Expected tracking**: For good error messages
5. **Clean separation**: Parsing vs semantic analysis
6. **Performance focus**: Careful memory management

---

## Rust Patterns for Parsers

Rust's type system and idioms enable elegant parser designs. Here are patterns that work well.

### The Option Dance

Parsing either succeeds (Some) or fails (None). The `?` operator makes this elegant:

```rust
fn parse_let(&mut self) -> Option<Stmt> {
    self.expect(TokenKind::Let)?;
    let name = self.parse_ident()?;
    self.expect(TokenKind::Equals)?;
    let value = self.parse_expr()?;
    self.expect(TokenKind::Semicolon)?;

    Some(Stmt::Let { name, value })
}
```

Each `?` returns None if the parse fails, bubbling up naturally.

### Builder Pattern for AST Construction

Separate parsing from AST allocation:

```rust
struct AstBuilder {
    ast: Ast,
    span_stack: Vec<Span>,
}

impl AstBuilder {
    fn start(&mut self, span: Span) {
        self.span_stack.push(span);
    }

    fn finish_expr(&mut self, expr: Expr, end: Span) -> Id<Expr> {
        let start = self.span_stack.pop().unwrap();
        let span = start.merge(&end);
        self.ast.alloc_expr_with_span(expr, span)
    }
}

// Usage:
fn parse_binary(&mut self) -> Option<Id<Expr>> {
    self.builder.start(self.current_span());

    let lhs = self.parse_unary()?;
    let op = self.parse_op()?;
    let rhs = self.parse_binary()?;

    Some(self.builder.finish_expr(
        Expr::Binary { lhs, op, rhs },
        self.previous_span()
    ))
}
```

### Combinator Functions

Create reusable parsing helpers:

```rust
impl Parser<'_> {
    /// Parse a comma-separated list with delimiters
    fn delimited<T>(
        &mut self,
        open: TokenKind,
        close: TokenKind,
        parse_element: impl Fn(&mut Self) -> Option<T>,
    ) -> Option<Vec<T>> {
        self.expect(open)?;

        let mut items = Vec::new();

        while !self.at(close) && !self.at_eof() {
            items.push(parse_element(self)?);

            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        self.expect(close)?;
        Some(items)
    }

    /// Parse a separated list without delimiters
    fn separated<T>(
        &mut self,
        sep: TokenKind,
        parse_element: impl Fn(&mut Self) -> Option<T>,
    ) -> Vec<T> {
        let mut items = Vec::new();

        if let Some(first) = parse_element(self) {
            items.push(first);

            while self.eat(sep) {
                if let Some(item) = parse_element(self) {
                    items.push(item);
                }
            }
        }

        items
    }

    /// Try to parse, backtracking on failure
    fn try_parse<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let state = self.save();
        let error_count = self.errors.len();

        match f(self) {
            Some(result) => Some(result),
            None => {
                self.restore(state);
                self.errors.truncate(error_count);
                None
            }
        }
    }
}
```

### Type-State for Parsing Phases

Use types to enforce that certain phases have completed:

```rust
struct Parsed<'src> {
    ast: Ast,
    errors: Vec<ParseError>,
}

struct Analyzed<'src> {
    ast: Ast,
    captures: HashMap<Id<Expr>, Vec<Capture>>,
}

impl<'src> Parsed<'src> {
    fn analyze(self) -> Analyzed<'src> {
        let captures = upvar_analysis(&self.ast);
        Analyzed { ast: self.ast, captures }
    }
}

// Can't do analysis without parsing first!
```

### Arenas for AST Nodes

Use arena allocation for efficient, cache-friendly ASTs:

```rust
struct Ast {
    exprs: Arena<Expr>,
    stmts: Arena<Stmt>,
    funcs: Arena<Func>,
}

// Nodes reference each other by ID, not pointer
enum Expr {
    Binary {
        op: BinOp,
        lhs: Id<Expr>,  // Arena ID, not Box
        rhs: Id<Expr>,
    },
    // ...
}
```

Benefits:
- All nodes contiguous in memory
- No reference counting overhead
- No lifetime complexity
- Easy serialization
- Efficient traversal

### Error Type Design

Create structured errors that can generate nice diagnostics:

```rust
struct ParseError {
    kind: ParseErrorKind,
    span: Span,
    expected: Vec<TokenKind>,
    found: TokenKind,
}

enum ParseErrorKind {
    UnexpectedToken,
    UnmatchedDelimiter { open: Span },
    InvalidExpression,
    MissingSemicolon,
}

impl ParseError {
    fn to_diagnostic(&self, source: &Source) -> Diagnostic {
        let msg = match &self.kind {
            ParseErrorKind::UnexpectedToken => {
                if self.expected.len() == 1 {
                    format!(
                        "expected `{}`, found `{}`",
                        self.expected[0],
                        self.found
                    )
                } else {
                    format!(
                        "expected one of {}, found `{}`",
                        self.expected.iter().map(|t| format!("`{}`", t)).collect::<Vec<_>>().join(", "),
                        self.found
                    )
                }
            }
            ParseErrorKind::UnmatchedDelimiter { open } => {
                format!("unmatched delimiter")
            }
            // ...
        };

        Diagnostic::error(msg)
            .with_label(Label::primary(self.span, "here"))
    }
}
```

---

## Architecture Design: Separating Concerns

A well-structured parser separates several concerns that are often mixed together.

### Lexer vs Parser

The lexer handles:
- Character-level concerns
- Tokenization
- Whitespace/comment handling
- Literal parsing (strings, numbers)
- Span tracking

The parser handles:
- Token-level concerns
- Grammar rules
- AST construction
- Precedence/associativity
- Error recovery

Clean separation means you can test them independently and swap implementations.

### Parsing vs Validation

Some checks belong in the parser, others in later phases:

**Parser's job**:
- Syntactic structure
- Basic well-formedness
- Building the AST

**Not parser's job**:
- Type checking
- Name resolution
- Semantic analysis

Example: parsing `let x: Foo = 1;` should succeed even if `Foo` doesn't exist. That's a name resolution error, not a parse error.

### AST Building vs Span Tracking

Mixing them clutters the code:

```rust
// Cluttered:
fn parse_binary(&mut self) -> Expr {
    let start = self.current_span();
    let lhs = self.parse_unary();
    let op = self.parse_op();
    let rhs = self.parse_unary();
    let span = start.merge(&self.previous_span());
    self.ast.alloc_with_span(Expr::Binary { lhs, op, rhs }, span)
}

// Cleaner with builder:
fn parse_binary(&mut self) -> Id<Expr> {
    self.builder.start_span(self.current_span());
    let lhs = self.parse_unary()?;
    let op = self.parse_op()?;
    let rhs = self.parse_unary()?;
    self.builder.finish_expr(Expr::Binary { lhs, op, rhs })
}
```

### Grammar Table vs Parsing Logic

Centralize grammar knowledge:

```rust
// grammar.rs - All precedence info in one place
pub struct Grammar;

impl Grammar {
    pub const ASSIGNMENT: u8 = 1;
    pub const OR: u8 = 2;
    pub const AND: u8 = 3;
    pub const EQUALITY: u8 = 4;
    pub const COMPARISON: u8 = 5;
    pub const ADDITIVE: u8 = 6;
    pub const MULTIPLICATIVE: u8 = 7;
    pub const UNARY: u8 = 8;
    pub const CALL: u8 = 9;

    pub fn infix_op(kind: TokenKind) -> Option<(BinOp, OpInfo)> {
        // ...
    }
}

// expr.rs - Uses the grammar, doesn't define it
fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
    let Some((op, info)) = Grammar::infix_op(self.peek()) else { break };
    // ...
}
```

### Proposed File Organization

```
src/parser/
    mod.rs          # Public API, Parser struct, token primitives
    grammar.rs      # Binding power table, operator classification
    builder.rs      # AstBuilder for decoupled construction
    recovery.rs     # Error recovery strategies
    combinators.rs  # Reusable parsing helpers
    error.rs        # ParseError types and diagnostics

    # Parse functions by language construct:
    expr.rs         # Expression parsing (Pratt)
    stmt.rs         # Statement parsing
    decl.rs         # Declaration parsing
    ty.rs           # Type parsing
    pattern.rs      # Pattern parsing (for match, let)
```

Each file has a clear responsibility. Adding new syntax means adding to the appropriate file.

---

## Implementation Walkthrough

Let's build a parser for a small language step by step.

### Step 1: Define the Grammar

```
program     → declaration*

declaration → fn_decl | let_stmt

fn_decl     → 'fn' IDENT '(' params? ')' ('->' type)? block

params      → param (',' param)*
param       → IDENT ':' type

let_stmt    → 'let' 'mut'? IDENT (':' type)? '=' expr ';'

block       → '{' statement* expr? '}'

statement   → let_stmt
            | expr ';'
            | 'while' expr block
            | 'loop' block

expr        → assignment

assignment  → equality ('=' assignment)?

equality    → comparison (('==' | '!=') comparison)*

comparison  → additive (('<' | '>' | '<=' | '>=') additive)*

additive    → multiplicative (('+' | '-') multiplicative)*

multiplicative → unary (('*' | '/') unary)*

unary       → ('!' | '-' | '&' 'mut'? | '*') unary
            | postfix

postfix     → primary ('(' args? ')' | '.' IDENT | '[' expr ']')*

primary     → INT | BOOL | STRING | IDENT
            | '(' expr ')'
            | 'if' expr block ('else' block)?
            | block
```

### Step 2: Token Primitives

```rust
// parser/mod.rs

pub struct Parser<'src> {
    tokens: Vec<Token<'src>>,
    pos: usize,
    builder: AstBuilder,
    errors: Vec<ParseError>,
    in_recovery: bool,
}

impl<'src> Parser<'src> {
    pub fn new(tokens: Vec<Token<'src>>) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: AstBuilder::new(),
            errors: Vec::new(),
            in_recovery: false,
        }
    }

    // --- Token inspection ---

    fn peek(&self) -> TokenKind {
        self.tokens.get(self.pos)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn peek_token(&self) -> &Token<'src> {
        &self.tokens[self.pos]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek() == kind
    }

    fn at_eof(&self) -> bool {
        self.at(TokenKind::Eof)
    }

    fn current_span(&self) -> Span {
        self.peek_token().span.clone()
    }

    fn previous_span(&self) -> Span {
        self.tokens[self.pos.saturating_sub(1)].span.clone()
    }

    // --- Token consumption ---

    fn advance(&mut self) {
        if !self.at_eof() {
            self.pos += 1;
        }
        self.skip_trivia();
    }

    fn skip_trivia(&mut self) {
        while matches!(self.peek(), TokenKind::Whitespace | TokenKind::Comment) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            self.in_recovery = false;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Span> {
        if self.at(kind) {
            let span = self.current_span();
            self.advance();
            self.in_recovery = false;
            Some(span)
        } else {
            self.error_expected(&[kind]);
            None
        }
    }

    // --- Error handling ---

    fn error(&mut self, message: String) {
        if !self.in_recovery {
            self.errors.push(ParseError {
                message,
                span: self.current_span(),
            });
        }
        self.in_recovery = true;
    }

    fn error_expected(&mut self, expected: &[TokenKind]) {
        let msg = if expected.len() == 1 {
            format!("expected {:?}", expected[0])
        } else {
            format!("expected one of {:?}", expected)
        };
        self.error(msg);
    }
}
```

### Step 3: The Binding Power Table

```rust
// parser/grammar.rs

#[derive(Clone, Copy)]
pub enum Assoc {
    Left,
    Right,
    None,
}

pub struct OpInfo {
    pub precedence: u8,
    pub assoc: Assoc,
}

impl OpInfo {
    pub fn binding_power(self) -> (u8, u8) {
        let base = self.precedence * 2;
        match self.assoc {
            Assoc::Left => (base, base + 1),
            Assoc::Right => (base + 1, base),
            Assoc::None => (base, base),
        }
    }
}

pub struct Grammar;

impl Grammar {
    pub const ASSIGNMENT: u8 = 1;
    pub const OR: u8 = 2;
    pub const AND: u8 = 3;
    pub const EQUALITY: u8 = 4;
    pub const COMPARISON: u8 = 5;
    pub const ADDITIVE: u8 = 6;
    pub const MULTIPLICATIVE: u8 = 7;
    pub const UNARY: u8 = 8;
    pub const POSTFIX: u8 = 9;

    pub fn infix_op(kind: TokenKind) -> Option<(BinOp, OpInfo)> {
        let (op, prec, assoc) = match kind {
            TokenKind::Equals => (BinOp::Assign, Self::ASSIGNMENT, Assoc::Right),

            TokenKind::PipePipe => (BinOp::Or, Self::OR, Assoc::Left),
            TokenKind::AmpAmp => (BinOp::And, Self::AND, Assoc::Left),

            TokenKind::EqEq => (BinOp::Eq, Self::EQUALITY, Assoc::Left),
            TokenKind::BangEq => (BinOp::NotEq, Self::EQUALITY, Assoc::Left),

            TokenKind::Lt => (BinOp::Lt, Self::COMPARISON, Assoc::None),
            TokenKind::Gt => (BinOp::Gt, Self::COMPARISON, Assoc::None),
            TokenKind::LtEq => (BinOp::LtEq, Self::COMPARISON, Assoc::None),
            TokenKind::GtEq => (BinOp::GtEq, Self::COMPARISON, Assoc::None),

            TokenKind::Plus => (BinOp::Add, Self::ADDITIVE, Assoc::Left),
            TokenKind::Minus => (BinOp::Sub, Self::ADDITIVE, Assoc::Left),

            TokenKind::Star => (BinOp::Mul, Self::MULTIPLICATIVE, Assoc::Left),
            TokenKind::Slash => (BinOp::Div, Self::MULTIPLICATIVE, Assoc::Left),

            _ => return None,
        };

        Some((op, OpInfo { precedence: prec, assoc }))
    }

    pub fn prefix_bp(kind: TokenKind) -> Option<u8> {
        match kind {
            TokenKind::Bang | TokenKind::Minus |
            TokenKind::Ampersand | TokenKind::Star => {
                Some(Self::UNARY * 2 + 1)
            }
            _ => None,
        }
    }
}
```

### Step 4: Expression Parsing (Pratt)

```rust
// parser/expr.rs

impl<'src> Parser<'src> {
    pub fn parse_expr(&mut self) -> Option<Id<Expr>> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.builder.start_span(start);

        // Parse prefix or atom
        let mut lhs = self.parse_prefix_or_atom()?;

        loop {
            // Check for postfix operators
            let postfix_bp = Grammar::POSTFIX * 2;
            match self.peek() {
                TokenKind::OpenParen if postfix_bp >= min_bp => {
                    lhs = self.parse_call(lhs)?;
                    continue;
                }
                TokenKind::Dot if postfix_bp >= min_bp => {
                    lhs = self.parse_field_access(lhs)?;
                    continue;
                }
                TokenKind::OpenBracket if postfix_bp >= min_bp => {
                    lhs = self.parse_index(lhs)?;
                    continue;
                }
                _ => {}
            }

            // Check for infix operators
            let Some((op, info)) = Grammar::infix_op(self.peek()) else {
                break;
            };

            let (lbp, rbp) = info.binding_power();
            if lbp < min_bp {
                break;
            }

            self.advance(); // Consume operator
            let rhs = self.parse_expr_bp(rbp)?;

            lhs = self.builder.alloc_expr(
                Expr::Binary { op, lhs, rhs },
                self.previous_span(),
            );
        }

        Some(lhs)
    }

    fn parse_prefix_or_atom(&mut self) -> Option<Id<Expr>> {
        match self.peek() {
            TokenKind::Bang => {
                let start = self.current_span();
                self.advance();
                let bp = Grammar::prefix_bp(TokenKind::Bang).unwrap();
                let operand = self.parse_expr_bp(bp)?;
                Some(self.builder.alloc_expr(
                    Expr::Not { expr: operand },
                    start.merge(&self.previous_span()),
                ))
            }

            TokenKind::Minus => {
                let start = self.current_span();
                self.advance();
                let bp = Grammar::prefix_bp(TokenKind::Minus).unwrap();
                let operand = self.parse_expr_bp(bp)?;
                Some(self.builder.alloc_expr(
                    Expr::Negate { expr: operand },
                    start.merge(&self.previous_span()),
                ))
            }

            TokenKind::Ampersand => {
                let start = self.current_span();
                self.advance();
                let mutable = self.eat(TokenKind::Mut);
                let bp = Grammar::prefix_bp(TokenKind::Ampersand).unwrap();
                let operand = self.parse_expr_bp(bp)?;
                Some(self.builder.alloc_expr(
                    Expr::Borrow { mutable, expr: operand },
                    start.merge(&self.previous_span()),
                ))
            }

            TokenKind::Star => {
                let start = self.current_span();
                self.advance();
                let bp = Grammar::prefix_bp(TokenKind::Star).unwrap();
                let operand = self.parse_expr_bp(bp)?;
                Some(self.builder.alloc_expr(
                    Expr::Deref { expr: operand },
                    start.merge(&self.previous_span()),
                ))
            }

            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();

        match self.peek() {
            TokenKind::Int => {
                let value: i32 = self.peek_token().text.parse().ok()?;
                self.advance();
                Some(self.builder.alloc_expr(Expr::I32(value), start))
            }

            TokenKind::True => {
                self.advance();
                Some(self.builder.alloc_expr(Expr::Bool(true), start))
            }

            TokenKind::False => {
                self.advance();
                Some(self.builder.alloc_expr(Expr::Bool(false), start))
            }

            TokenKind::String => {
                let text = self.peek_token().text;
                // Remove quotes
                let value = text[1..text.len()-1].into();
                self.advance();
                Some(self.builder.alloc_expr(Expr::String(value), start))
            }

            TokenKind::Ident => {
                let name = self.parse_ident()?;
                Some(self.builder.alloc_expr(Expr::Var(name), start))
            }

            TokenKind::OpenParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(TokenKind::CloseParen)?;
                Some(inner)
            }

            TokenKind::If => self.parse_if(),

            TokenKind::OpenBrace => self.parse_block(),

            _ => {
                self.error("expected expression".into());
                None
            }
        }
    }

    fn parse_call(&mut self, func: Id<Expr>) -> Option<Id<Expr>> {
        let start = self.builder.ast.get_span(&func);
        self.expect(TokenKind::OpenParen)?;

        let args = self.parse_args()?;

        self.expect(TokenKind::CloseParen)?;

        Some(self.builder.alloc_expr(
            Expr::Call { func, args },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_args(&mut self) -> Option<Vec<Id<Expr>>> {
        let mut args = Vec::new();

        if !self.at(TokenKind::CloseParen) {
            args.push(self.parse_expr()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break; // Trailing comma
                }
                args.push(self.parse_expr()?);
            }
        }

        Some(args)
    }

    fn parse_field_access(&mut self, object: Id<Expr>) -> Option<Id<Expr>> {
        let start = self.builder.ast.get_span(&object);
        self.expect(TokenKind::Dot)?;
        let field = self.parse_ident()?;

        Some(self.builder.alloc_expr(
            Expr::FieldAccess { object, field },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_index(&mut self, array: Id<Expr>) -> Option<Id<Expr>> {
        let start = self.builder.ast.get_span(&array);
        self.expect(TokenKind::OpenBracket)?;
        let index = self.parse_expr()?;
        self.expect(TokenKind::CloseBracket)?;

        Some(self.builder.alloc_expr(
            Expr::Index { array, index },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_if(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::If)?;

        let condition = self.parse_expr()?;
        let then_block = self.parse_block()?;

        let else_block = if self.eat(TokenKind::Else) {
            Some(self.parse_block()?)
        } else {
            None
        };

        Some(self.builder.alloc_expr(
            Expr::If { condition, then_block, else_block },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_block(&mut self) -> Option<Id<Expr>> {
        let start = self.current_span();
        self.expect(TokenKind::OpenBrace)?;

        let mut stmts = Vec::new();
        let mut value = None;

        while !self.at(TokenKind::CloseBrace) && !self.at_eof() {
            match self.parse_stmt_or_expr() {
                StmtOrExpr::Stmt(stmt) => stmts.push(stmt),
                StmtOrExpr::Expr(expr) => {
                    if self.at(TokenKind::CloseBrace) {
                        value = Some(expr);
                    } else {
                        self.error("expected `;` or `}`".into());
                        break;
                    }
                }
                StmtOrExpr::Error => {
                    self.recover(RecoveryLevel::Statement);
                }
            }
        }

        self.expect(TokenKind::CloseBrace)?;

        Some(self.builder.alloc_expr(
            Expr::Block { stmts, value },
            start.merge(&self.previous_span()),
        ))
    }
}
```

### Step 5: Statement Parsing

```rust
// parser/stmt.rs

enum StmtOrExpr {
    Stmt(Id<Stmt>),
    Expr(Id<Expr>),
    Error,
}

impl<'src> Parser<'src> {
    fn parse_stmt_or_expr(&mut self) -> StmtOrExpr {
        // Try statement keywords first
        match self.peek() {
            TokenKind::Let => {
                match self.parse_let() {
                    Some(stmt) => StmtOrExpr::Stmt(stmt),
                    None => StmtOrExpr::Error,
                }
            }
            TokenKind::While => {
                match self.parse_while() {
                    Some(stmt) => StmtOrExpr::Stmt(stmt),
                    None => StmtOrExpr::Error,
                }
            }
            TokenKind::Loop => {
                match self.parse_loop() {
                    Some(stmt) => StmtOrExpr::Stmt(stmt),
                    None => StmtOrExpr::Error,
                }
            }
            _ => {
                // Try expression
                match self.parse_expr() {
                    Some(expr) => {
                        if self.eat(TokenKind::Semicolon) {
                            let stmt = self.builder.alloc_stmt(Stmt::Expr { expr });
                            StmtOrExpr::Stmt(stmt)
                        } else {
                            StmtOrExpr::Expr(expr)
                        }
                    }
                    None => StmtOrExpr::Error,
                }
            }
        }
    }

    fn parse_let(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Let)?;

        let mutable = self.eat(TokenKind::Mut);
        let name = self.parse_ident()?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(TokenKind::Equals)?;
        let value = self.parse_expr()?;
        self.expect(TokenKind::Semicolon)?;

        Some(self.builder.alloc_stmt_with_span(
            Stmt::Let { name, mutable, ty, value },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_while(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::While)?;

        let condition = self.parse_expr()?;
        let body = self.parse_block()?;

        Some(self.builder.alloc_stmt_with_span(
            Stmt::While { condition, body },
            start.merge(&self.previous_span()),
        ))
    }

    fn parse_loop(&mut self) -> Option<Id<Stmt>> {
        let start = self.current_span();
        self.expect(TokenKind::Loop)?;

        let body = self.parse_block()?;

        Some(self.builder.alloc_stmt_with_span(
            Stmt::Loop { body },
            start.merge(&self.previous_span()),
        ))
    }
}
```

### Step 6: Declaration Parsing

```rust
// parser/decl.rs

impl<'src> Parser<'src> {
    pub fn parse_program(&mut self) {
        while !self.at_eof() {
            if let Some(decl) = self.parse_decl() {
                self.builder.add_decl(decl);
            } else {
                self.recover(RecoveryLevel::Declaration);
            }
        }
    }

    fn parse_decl(&mut self) -> Option<Decl> {
        match self.peek() {
            TokenKind::Fn => Some(Decl::Func(self.parse_function()?)),
            TokenKind::Struct => Some(Decl::Struct(self.parse_struct()?)),
            TokenKind::Extern => Some(Decl::Extern(self.parse_extern()?)),
            _ => {
                self.error_expected(&[TokenKind::Fn, TokenKind::Struct, TokenKind::Extern]);
                None
            }
        }
    }

    fn parse_function(&mut self) -> Option<Id<Func>> {
        let start = self.current_span();
        self.expect(TokenKind::Fn)?;

        let name = self.parse_ident()?;

        // Type parameters: fn foo<T, U>
        let type_params = if self.eat(TokenKind::Lt) {
            let params = self.parse_type_params()?;
            self.expect(TokenKind::Gt)?;
            params
        } else {
            Vec::new()
        };

        // Parameters: (x: i32, y: bool)
        self.expect(TokenKind::OpenParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::CloseParen)?;

        // Return type: -> i32
        let return_type = if self.eat(TokenKind::Arrow) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Body
        let body = self.parse_block()?;

        Some(self.builder.alloc_func(Func {
            name,
            type_params,
            params,
            return_type,
            body,
        }))
    }

    fn parse_params(&mut self) -> Option<Vec<FuncParam>> {
        let mut params = Vec::new();

        if !self.at(TokenKind::CloseParen) {
            params.push(self.parse_param()?);

            while self.eat(TokenKind::Comma) {
                if self.at(TokenKind::CloseParen) {
                    break;
                }
                params.push(self.parse_param()?);
            }
        }

        Some(params)
    }

    fn parse_param(&mut self) -> Option<FuncParam> {
        let name = self.parse_ident()?;
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type()?;

        Some(FuncParam { name, ty })
    }
}
```

### Step 7: Error Recovery

```rust
// parser/recovery.rs

#[derive(Clone, Copy)]
pub enum RecoveryLevel {
    Expression,
    Statement,
    Declaration,
}

impl<'src> Parser<'src> {
    pub fn recover(&mut self, level: RecoveryLevel) {
        let sync_tokens: &[TokenKind] = match level {
            RecoveryLevel::Expression => &[
                TokenKind::Semicolon,
                TokenKind::Comma,
                TokenKind::CloseParen,
                TokenKind::CloseBracket,
                TokenKind::CloseBrace,
            ],
            RecoveryLevel::Statement => &[
                TokenKind::Semicolon,
                TokenKind::CloseBrace,
                TokenKind::Let,
                TokenKind::While,
                TokenKind::Loop,
                TokenKind::If,
            ],
            RecoveryLevel::Declaration => &[
                TokenKind::Fn,
                TokenKind::Struct,
                TokenKind::Extern,
                TokenKind::CloseBrace,
            ],
        };

        while !self.at_eof() {
            if sync_tokens.contains(&self.peek()) {
                // For some tokens, consume them as part of recovery
                if self.at(TokenKind::Semicolon) {
                    self.advance();
                }
                self.in_recovery = false;
                return;
            }

            // Skip balanced delimiters
            match self.peek() {
                TokenKind::OpenBrace => {
                    self.skip_balanced(TokenKind::OpenBrace, TokenKind::CloseBrace);
                }
                TokenKind::OpenParen => {
                    self.skip_balanced(TokenKind::OpenParen, TokenKind::CloseParen);
                }
                TokenKind::OpenBracket => {
                    self.skip_balanced(TokenKind::OpenBracket, TokenKind::CloseBracket);
                }
                _ => self.advance(),
            }
        }
    }

    fn skip_balanced(&mut self, open: TokenKind, close: TokenKind) {
        assert!(self.at(open));
        let mut depth = 0;

        loop {
            if self.at(open) {
                depth += 1;
            } else if self.at(close) {
                depth -= 1;
                if depth == 0 {
                    self.advance();
                    return;
                }
            } else if self.at_eof() {
                return;
            }
            self.advance();
        }
    }
}
```

### Step 8: The Public API

```rust
// parser/mod.rs

pub fn parse(source: Arc<Source>) -> (Ast, Vec<ParseError>) {
    let tokens = lexer::lex(&source);
    let mut parser = Parser::new(tokens);

    parser.parse_program();

    let ast = parser.builder.into_ast();
    let errors = parser.errors;

    (ast, errors)
}
```

---

## Testing Strategy

A parser needs comprehensive testing to catch edge cases and regressions.

### Unit Tests for Primitives

```rust
#[test]
fn test_peek_and_advance() {
    let tokens = lex("1 + 2");
    let mut parser = Parser::new(tokens);

    assert_eq!(parser.peek(), TokenKind::Int);
    parser.advance();
    assert_eq!(parser.peek(), TokenKind::Plus);
    parser.advance();
    assert_eq!(parser.peek(), TokenKind::Int);
    parser.advance();
    assert_eq!(parser.peek(), TokenKind::Eof);
}

#[test]
fn test_eat() {
    let tokens = lex("1 +");
    let mut parser = Parser::new(tokens);

    assert!(parser.eat(TokenKind::Int));
    assert!(!parser.eat(TokenKind::Int)); // Next is Plus
    assert!(parser.eat(TokenKind::Plus));
}

#[test]
fn test_expect() {
    let tokens = lex("1 +");
    let mut parser = Parser::new(tokens);

    assert!(parser.expect(TokenKind::Int).is_some());
    assert!(parser.expect(TokenKind::Minus).is_none());
    assert!(!parser.errors.is_empty());
}
```

### Expression Parsing Tests

```rust
#[test]
fn test_precedence() {
    let ast = parse_expr("1 + 2 * 3");
    assert_eq!(ast.to_string(), "(1 + (2 * 3))");
}

#[test]
fn test_left_associativity() {
    let ast = parse_expr("1 - 2 - 3");
    assert_eq!(ast.to_string(), "((1 - 2) - 3)");
}

#[test]
fn test_right_associativity() {
    let ast = parse_expr("a = b = c");
    assert_eq!(ast.to_string(), "(a = (b = c))");
}

#[test]
fn test_unary_prefix() {
    let ast = parse_expr("-1");
    assert_eq!(ast.to_string(), "(-1)");
}

#[test]
fn test_unary_precedence() {
    let ast = parse_expr("-1 * 2");
    assert_eq!(ast.to_string(), "((-1) * 2)");
}

#[test]
fn test_call() {
    let ast = parse_expr("foo(1, 2)");
    assert_eq!(ast.to_string(), "foo(1, 2)");
}

#[test]
fn test_chained_calls() {
    let ast = parse_expr("a.b().c");
    assert_eq!(ast.to_string(), "((a.b)().c)");
}

#[test]
fn test_complex_expression() {
    let ast = parse_expr("a + b * c.d(e, f) - g");
    assert_eq!(ast.to_string(), "((a + (b * ((c.d)(e, f)))) - g)");
}
```

### Statement Parsing Tests

```rust
#[test]
fn test_let_simple() {
    let stmt = parse_stmt("let x = 1;");
    assert!(matches!(stmt, Stmt::Let { mutable: false, ty: None, .. }));
}

#[test]
fn test_let_mutable() {
    let stmt = parse_stmt("let mut x = 1;");
    assert!(matches!(stmt, Stmt::Let { mutable: true, .. }));
}

#[test]
fn test_let_with_type() {
    let stmt = parse_stmt("let x: i32 = 1;");
    assert!(matches!(stmt, Stmt::Let { ty: Some(_), .. }));
}

#[test]
fn test_while() {
    let stmt = parse_stmt("while x < 10 { x = x + 1; }");
    assert!(matches!(stmt, Stmt::While { .. }));
}
```

### Error Recovery Tests

```rust
#[test]
fn test_missing_semicolon_recovery() {
    let (ast, errors) = parse("let x = 1\nlet y = 2;");

    assert_eq!(errors.len(), 1);
    assert!(errors[0].message.contains("semicolon"));

    // Should still parse both statements
    assert_eq!(ast.stmts.len(), 2);
}

#[test]
fn test_unmatched_paren_recovery() {
    let (ast, errors) = parse("fn foo(x: i32 { }");

    assert!(!errors.is_empty());
    // Should still recognize it's a function
    assert_eq!(ast.funcs.len(), 1);
}

#[test]
fn test_cascade_suppression() {
    let (_, errors) = parse("let x = + ;");

    // Should report one error, not multiple
    assert!(errors.len() <= 2);
}
```

### Snapshot Testing

For complex inputs, use snapshot testing:

```rust
#[test]
fn snapshot_complex_function() {
    let input = r#"
        fn fib(n: i32) -> i32 {
            if n <= 1 {
                n
            } else {
                fib(n - 1) + fib(n - 2)
            }
        }
    "#;

    let (ast, errors) = parse(input);

    assert!(errors.is_empty());
    insta::assert_snapshot!(format_ast(&ast));
}
```

### Fuzzing

Use `cargo-fuzz` to find edge cases:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        // Should never panic, even on garbage input
        let _ = parser::parse(s);
    }
});
```

---

## Further Reading

### Books

**Compilers: Principles, Techniques, and Tools** (The Dragon Book)
*Aho, Lam, Sethi, Ullman*
The classic. Comprehensive coverage of parsing theory.

**Crafting Interpreters**
*Robert Nystrom*
Practical, approachable. Free online at craftinginterpreters.com.

**Engineering a Compiler**
*Cooper, Torczon*
Modern treatment with excellent parsing coverage.

**Language Implementation Patterns**
*Terence Parr*
Practical patterns from the creator of ANTLR.

### Papers

**"Top Down Operator Precedence"**
*Vaughan Pratt, 1973*
The original Pratt parsing paper.

**"Simple but Powerful Pratt Parsing"**
*Bob Nystrom (blog post)*
Excellent modern explanation of Pratt parsing.

**"Parsing Techniques: A Practical Guide"**
*Dick Grune, Ceriel Jacobs*
Comprehensive survey of parsing algorithms.

### Compiler Source Code

**rustc**
https://github.com/rust-lang/rust/tree/master/compiler/rustc_parse
Rust's parser. See `parser/expr.rs` for Pratt parsing.

**Clang**
https://github.com/llvm/llvm-project/tree/main/clang/lib/Parse
C/C++ parser with tentative parsing.

**Go**
https://github.com/golang/go/tree/master/src/cmd/compile/internal/syntax
Go's parser. Notably simple.

**Swift**
https://github.com/apple/swift/tree/main/lib/Parse
Swift's parser with sophisticated error recovery.

### Blog Posts

**"Pratt Parsers: Expression Parsing Made Easy"**
https://journal.stuffwithstuff.com/2011/03/19/pratt-parsers-expression-parsing-made-easy/
Bob Nystrom's introduction to Pratt parsing.

**"Simple Top-Down Parsing in Python"**
http://effbot.org/zone/simple-top-down-parsing.htm
Fredrik Lundh's Python implementation.

**"Parsing Expressions by Precedence Climbing"**
https://eli.thegreenplace.net/2012/08/02/parsing-expressions-by-precedence-climbing
Eli Bendersky's take on precedence parsing.

**"From Precedence Climbing to Pratt Parsing"**
https://www.engr.mun.ca/~theo/Misc/pratt_parsing.htm
Theodore Norvell's comparison of techniques.

### Tools

**Logos**
https://docs.rs/logos/
Fast lexer generator for Rust. Good for building tokens.

**chumsky**
https://docs.rs/chumsky/
Parser combinator library for Rust. Good for learning, though we're doing handwritten.

**tree-sitter**
https://tree-sitter.github.io/
Modern parser generator with excellent error recovery. Worth studying.

---

## Closing Thoughts

Parsing is where source code becomes structure. A good parser is invisible—developers don't notice it until it fails them. A great parser is helpful—it catches errors, explains them clearly, and suggests fixes.

The techniques we've covered:

- **Recursive descent** for declarative structure
- **Pratt parsing** for elegant expression handling
- **Multi-level recovery** for robustness
- **Clean architecture** for maintainability

These aren't just theory. They're battle-tested in production compilers used by millions of developers.

When you build a parser:

1. **Start simple**: Get the happy path working first
2. **Add tests early**: Regression tests save debugging time
3. **Improve errors incrementally**: Each confusing error is a chance to help
4. **Study other compilers**: There's no shame in borrowing good ideas
5. **Measure twice, cut once**: Grammar design affects everything downstream

The parser is the first impression your language makes on developers. Make it a good one.

Happy parsing!

# som

> An idiot admires complexity, a genius admires simplicity.

```rust
enum breed:
  siamese
  persian
  maine_coon
  sphynx
  other ~ string

type cat:
  name ~ string
  age ~ number
  breed ~ breed

spec purrer:
  pur ~ fn (self) -> number

code purrer for cat:
  pur: (self) -> {
    12
  }
```

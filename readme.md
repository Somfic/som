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

```rust
type Name: 
  name ~ string;

type Age: 
  age ~ number;

enum Tail:
  none
  medium
  long;

type Cat with Name, Age:
  sleep_count ~ number
  tail_length ~ Tail.none;

spec Purrer:
  pur ~ fn(self) -> number;

code Purrer  Cat:
  fn pur(self) -> number {
    print`($"{self.name} ({self.age}) is purring");
  }

  fn sleep(self) {
    self.sleep_count++;
  };
```

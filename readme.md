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
struct Name: 
  name ~ string;

struct Age: age ~ number;

enum tail_length:
  short,
  medium,
  long;

type Cat
  tail_length ~ tail_length, 
  + Name, 
  + Age;

spec Purrer:
  pur ~ fn(self) -> number;

code Purrer for Cat:
  fn pur(self) -> number {
    print`($"{self.name} ({self.age}) is purring");
  };

```


```
install cmake
install strawberryperl

winget install Ninja-build.Ninja


cargo install llvmenv
llvmenv init
llvmenv entries
llvmenv build-entry 10.0.0
```

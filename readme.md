# som

> An idiot admires complexity, a genius admires simplicity.

```ts
struct person:
  name: string
  age: number

  age_in_days() -> number:
    let age_in_months = .age * 12;
    age_in_months * 30
  ;

  *new(name: string, age: number) -> self:
    self {
      name
      age
    }
  ;
;

let lucas = person::new('Lucas', 22);
lucas.age_in_days(); // 7920
```

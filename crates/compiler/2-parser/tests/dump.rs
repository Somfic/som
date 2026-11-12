//! The AST dump format. Other implementations of som (like a self-hosted one)
//! are compared against it, so changes to it should be deliberate.

use som_base::Source;
use std::sync::Arc;

fn dump(source: &str) -> String {
    let (ast, errors) = som_parser::parse(Arc::new(Source::from_raw(source)));
    assert!(errors.is_empty(), "{errors:?}");
    ast.dump()
}

#[test]
fn covers_every_construct() {
    let source = r#"
use std;
extern "m" { fn sqrt(x: f64) -> f64; }
struct P { x: i32, r: &'a mut P }
impl P { fn get(self: &P) -> i32 { self.x } }
fn id<T>(v: T) -> T { v }
fn main() -> i32 {
    let mut n: i32 = 0;
    while n < 3 { n = n + 1; }
    if !false { loop { } } else { std::exit(*&n); }
    let p = P { x: 1, r: &mut q };
    p.get() if true else 1.5
}
"#;
    assert_eq!(
        dump(source),
        r#"module ""
  use std
  extern "m"
    fn sqrt(x: f64) -> f64
  struct P
    field x: i32
    field r: &'a mut P
  impl P
    fn get(self: &P) -> i32
      block
        value
          field x
            var self
  fn id<T>(v: T) -> T
    block
      value
        var v
  fn main() -> i32
    block
      let mut n: i32
        int 0
      while
        binary <
          var n
          int 3
        do
          assign
            var n
            binary +
              var n
              int 1
      if
        not
          bool false
        then
          loop
        else
          call std::exit
            deref
              borrow
                var n
      let p
        construct P
          x:
            int 1
          r:
            borrow mut
              var q
      value
        conditional
          condition
            bool true
          then
            method get
              var p
          else
            float 1.5
"#
    );
}

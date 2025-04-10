use crate::run;

mod binary;
mod block;
mod conditional;
mod function;
mod group;
mod loops;
mod unary;
mod variables;

pub fn run_and_assert(source_code: impl Into<String>, expected: i64) {
    assert_eq!(run(source_code), expected);
}

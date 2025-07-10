use crate::prelude::*;

mod arithmetic;
mod literals;
mod variables;

pub fn interpret(source: &str) -> i64 {
    run(miette::NamedSource::new("test", source.to_string()))
}

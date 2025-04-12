use crate::runner::run;

#[test]
fn binary() {
    run(include_str!("binary.som"));
}

#[test]
fn unary() {
    run(include_str!("unary.som"));
}

#[test]
fn variables() {
    run(include_str!("variables.som"));
}

#[test]
fn block() {
    run(include_str!("block.som"));
}

#[test]
fn conditional() {
    run(include_str!("conditional.som"));
}

#[test]
fn function() {
    run(include_str!("function.som"));
}

#[test]
fn group() {
    run(include_str!("group.som"));
}

#[test]
fn loops() {
    run(include_str!("loops.som"));
}

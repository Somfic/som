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
fn one_file() {
    run(include_str!("one_file.som"));
}

#[test]
fn type_alias() {
    run(include_str!("type_alias.som"));
}

#[test]
fn return_codes() {
    assert_eq!(1, run("1"));
    assert_eq!(0, run("1;"));
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

#[test]
fn types() {
    run(include_str!("types.som"));
}

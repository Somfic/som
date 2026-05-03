mod common;

#[test]
fn one_plus_one() {
    let result = som::compile_and_run("1 + 1").unwrap();
    assert_eq!(result, 2);
}

// #[test]
// fn test_compile() {
//     let result = common::compile("1 + 1");
//     assert_eq!(result.diagnostics.len(), 0);
//     insta::assert_debug_snapshot!(result.diagnostics);
// }

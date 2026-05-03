mod common;

#[test]
fn test_compile() {
    let result = common::compile("1 + 1");
    assert_eq!(result.diagnostics.len(), 0);
    insta::assert_debug_snapshot!(result.diagnostics);
}

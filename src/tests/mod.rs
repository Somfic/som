#[test]
fn pattern_matching() {
    let input = r#"
      match true {
        true  = "true"
        false = "false"
        _     = print("unknown command")
      }
    "#;


}

use som_base::diagnostics::{closest_match, edit_distance};

#[test]
fn edit_distance_counts_characters() {
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("", "abc"), 3);
    assert_eq!(edit_distance("same", "same"), 0);
    // `é` is two bytes but one character.
    assert_eq!(edit_distance("café", "cafe"), 1);
    // Swapping two neighbouring characters is one edit.
    assert_eq!(edit_distance("deom", "demo"), 1);
    assert_eq!(edit_distance("ab", "ba"), 1);
}

#[test]
fn closest_match_respects_the_limit() {
    let names = vec!["count".to_string(), "total".to_string()];
    assert_eq!(closest_match("cuont", &names, 2), Some("count"));
    assert_eq!(closest_match("zzzzz", &names, 2), None);
}

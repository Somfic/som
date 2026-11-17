use som_base::diagnostics::{closest_match, edit_distance, pascal_case, snake_case};

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

#[test]
fn snake_case_names() {
    for (c, som) in [
        ("InitWindow", "init_window"),
        ("GetFPS", "get_fps"),
        ("Vector2Add", "vector2_add"),
        ("DrawText3D", "draw_text3d"),
        ("LoadHTTPData", "load_http_data"),
        ("demo_add", "demo_add"),
        ("printf", "printf"),
        ("SDL_CreateWindow", "sdl_create_window"),
    ] {
        assert_eq!(snake_case(c), som, "{c}");
    }
}

#[test]
fn pascal_case_names() {
    for (c, som) in [
        ("ma_resampler_config", "MaResamplerConfig"),
        ("Color", "Color"),
        ("Vector2", "Vector2"),
        ("SDL_Rect", "SDLRect"),
        ("_private_tag", "PrivateTag"),
        ("point", "Point"),
    ] {
        assert_eq!(pascal_case(c), som, "{c}");
    }
}

#[test]
fn fixes_render_as_a_diff_of_the_changed_node() {
    use som_base::{Diagnostic, Fix, Label, Source, Span};
    use std::sync::Arc;

    let source = Arc::new(Source::from_raw("fn main() {\n    init_windw(1, 2);\n}\n"));
    let name = Span::from_range(16..26, source.clone());
    let diagnostic = Diagnostic::error("cannot find function `init_windw`")
        .with_label(Label::primary(name.clone(), "not found"))
        .with_fix(Fix::replace("change to `init_window`", name, "init_window"))
        .with_fix(Fix::add_line("add `use raylib;`", source, "use raylib;"));
    let text = diagnostic.render(false);
    for expected in [
        "   2 -     init_windw(1, 2);\n   2 +     init_window(1, 2);\n",
        "   1 + use raylib;\n   2 │   fn main() {\n",
    ] {
        assert!(text.contains(expected), "missing:\n{expected}\nin:\n{text}");
    }
}

#[test]
fn fixes_across_lines_render_each_line() {
    use som_base::{Diagnostic, Fix, Source, Span};
    use std::sync::Arc;

    let text =
        "extern \"raylib\" {\n    fn InitWindow();\n    fn CloseWindow();\n}\nfn main() {}\n";
    let source = Arc::new(Source::from_raw(text));
    let block = Span::from_range(0..text.find("}\n").unwrap() + 1, source.clone());
    let line = text.find("    fn Close").unwrap();
    let close = Span::from_range(line..text.find("}\n").unwrap(), source);
    let diagnostic = Diagnostic::warning("already declared")
        .with_fix(Fix::replace(
            "replace the block",
            block,
            "extern \"raylib\";",
        ))
        .with_fix(Fix::replace("remove `CloseWindow`", close, ""));
    let rendered = diagnostic.render(false);
    for expected in [
        "   1 - extern \"raylib\" {\n   2 -     fn InitWindow();\n   3 -     fn CloseWindow();\n   4 - }\n   1 + extern \"raylib\";\n",
    ] {
        assert!(
            rendered.contains(expected),
            "missing:\n{expected}\nin:\n{rendered}"
        );
    }
    // Removing whole lines leaves nothing to add, and the next line alone.
    assert!(
        rendered.ends_with("▪ remove `CloseWindow`\n   3 -     fn CloseWindow();\n"),
        "{rendered}"
    );
}

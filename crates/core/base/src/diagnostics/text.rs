/// How many single-character edits turn `a` into `b`: inserting, deleting or replacing
/// a character, or swapping two neighbouring ones (a common typo: `deom` for `demo`).
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // d[i][j]: the distance between the first i characters of a and the first j of b.
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// `InitWindow` → `init_window`, `GetFPS` → `get_fps`, `Vector2Add` → `vector2_add`,
/// `DrawText3D` → `draw_text3d`. Names already in snake_case stay as they are.
pub fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            let prev = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
            // A new word starts after a lowercase letter, or at the last capital of an
            // acronym or after a number when a lowercase letter follows: `FPSCounter`, `2Add`.
            let new_word = prev.is_ascii_lowercase()
                || ((prev.is_ascii_uppercase() || prev.is_ascii_digit()) && next_lower);
            if new_word && !out.ends_with('_') {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// `ma_resampler_config` → `MaResamplerConfig`, `Color` → `Color`,
/// `SDL_Rect` → `SDLRect`: each word's first letter capitalised and the underscores
/// dropped. Names already in PascalCase stay as they are.
pub fn pascal_case(name: &str) -> String {
    let words = name.split('_').filter(|w| !w.is_empty());
    let out: String = words
        .flat_map(|w| {
            let mut chars = w.chars();
            let first = chars.next().map(|c| c.to_ascii_uppercase());
            first.into_iter().chain(chars)
        })
        .collect();
    if out.is_empty() {
        name.to_string()
    } else {
        out
    }
}

/// Find the closest match from a list of candidates using edit distance.
/// Returns `None` if no candidate is close enough: within `max_dist` edits, and within a
/// third of `target`'s length (at least one), so a short name doesn't match one sharing only
/// half its letters (`null` isn't a typo of `nums`).
pub fn closest_match<'a>(
    target: &str,
    candidates: &'a [String],
    max_dist: usize,
) -> Option<&'a str> {
    let max_dist = max_dist.min((target.chars().count() / 3).max(1));
    candidates
        .iter()
        .map(|c| (c.as_str(), edit_distance(target, c)))
        .filter(|(_, d)| *d <= max_dist)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

/// Removes terminal colour codes from `text`.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip to the end of the escape sequence: a letter.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

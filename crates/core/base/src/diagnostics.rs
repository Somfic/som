use std::collections::HashSet;
use std::fmt::Display;

use owo_colors::OwoColorize;
/// A 24-bit colour, as used by the palette below.
pub use owo_colors::Rgb;

use std::sync::OnceLock;

use crate::Span;
use crate::message::Message;

// Catppuccin Mocha colors
pub const ROSEWATER: Rgb = Rgb(245, 224, 220);
pub const FLAMINGO: Rgb = Rgb(242, 205, 205);
pub const PINK: Rgb = Rgb(245, 194, 231);
pub const MAUVE: Rgb = Rgb(203, 166, 247);
pub const RED: Rgb = Rgb(243, 139, 168);
pub const MAROON: Rgb = Rgb(235, 160, 172);
pub const PEACH: Rgb = Rgb(250, 179, 135);
pub const YELLOW: Rgb = Rgb(249, 226, 175);
pub const GREEN: Rgb = Rgb(166, 227, 161);
pub const TEAL: Rgb = Rgb(148, 226, 213);
pub const SKY: Rgb = Rgb(137, 220, 235);
pub const SAPPHIRE: Rgb = Rgb(116, 199, 236);
pub const BLUE: Rgb = Rgb(137, 180, 250);
pub const LAVENDER: Rgb = Rgb(180, 190, 254);
pub const TEXT: Rgb = Rgb(205, 214, 244);
pub const SUBTEXT1: Rgb = Rgb(186, 194, 222);
pub const SUBTEXT0: Rgb = Rgb(166, 173, 200);
pub const OVERLAY2: Rgb = Rgb(147, 153, 178);
pub const OVERLAY1: Rgb = Rgb(127, 132, 156);
pub const OVERLAY0: Rgb = Rgb(108, 112, 134);
pub const SURFACE2: Rgb = Rgb(88, 91, 112);
pub const SURFACE1: Rgb = Rgb(69, 71, 90);
pub const SURFACE0: Rgb = Rgb(49, 50, 68);
pub const BASE: Rgb = Rgb(30, 30, 46);
pub const MANTLE: Rgb = Rgb(24, 24, 37);
pub const CRUST: Rgb = Rgb(17, 17, 27);

// Helper functions for applying colors
fn teal(text: impl std::fmt::Display) -> String {
    format!("{}", text.to_string().truecolor(TEAL.0, TEAL.1, TEAL.2))
}

fn subtext(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SUBTEXT0.0, SUBTEXT0.1, SUBTEXT0.2)
            .italic()
    )
}

fn surface(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SURFACE2.0, SURFACE2.1, SURFACE2.2)
    )
}

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

/// Find the closest match from a list of candidates using edit distance.
/// Returns `None` if no candidate is close enough (distance must be <= max_dist).
pub fn closest_match<'a>(
    target: &str,
    candidates: &'a [String],
    max_dist: usize,
) -> Option<&'a str> {
    candidates
        .iter()
        .map(|c| (c.as_str(), edit_distance(target, c)))
        .filter(|(_, d)| *d <= max_dist)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

/// Colours one line of source code for display in a diagnostic.
pub type Highlighter = fn(&str) -> String;

static HIGHLIGHTER: OnceLock<Highlighter> = OnceLock::new();

/// Install the syntax highlighter used for code snippets in diagnostics.
/// Without one, snippets are shown uncoloured.
pub fn set_highlighter(highlighter: Highlighter) {
    let _ = HIGHLIGHTER.set(highlighter);
}

pub(crate) fn syntax_highlight(line: &str) -> String {
    match HIGHLIGHTER.get() {
        Some(highlight) if !line.trim().is_empty() => highlight(line),
        _ => line.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Raw lines of output from elsewhere, like a C compiler's, shown as they are.
    pub trace: Vec<String>,
    pub message: Message,
    pub hints: Vec<Message>,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    fn new(severity: Severity, message: impl Into<Message>) -> Self {
        Self {
            severity,
            trace: vec![],
            message: message.into(),
            hints: vec![],
            labels: vec![],
        }
    }

    pub fn error(message: impl Into<Message>) -> Self {
        Self::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<Message>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// Something that went well, labelled with what happened: `built: example/example`.
    pub fn success(what: &'static str, message: impl Into<Message>) -> Self {
        Self::new(Severity::Success(what), message)
    }

    pub fn note(message: impl Into<Message>) -> Self {
        Self::new(Severity::Note, message)
    }

    pub fn with_label(mut self, label: impl Into<Label>) -> Self {
        self.labels.push(label.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<Message>) -> Self {
        self.hints.push(hint.into());
        self
    }

    pub fn with_trace(mut self, trace: impl Into<String>) -> Self {
        self.trace.push(trace.into());
        self
    }

    pub fn to_err<T>(self) -> Result<T, Self> {
        Err(self)
    }

    /// The diagnostic for a terminal (`color`), or as plain text with names and
    /// code between backticks.
    pub fn render(&self, color: bool) -> String {
        let text = Rendered {
            diagnostic: self,
            color,
        }
        .to_string();
        if color { text } else { strip_ansi(&text) }
    }
}

/// The gutter prefix: 4 chars for line number + space + pipe + space = "     │ "
const GUTTER: &str = "     ";

/// Render a group of labels from the same source file.
fn fmt_label_group(
    f: &mut std::fmt::Formatter<'_>,
    group: &[&Label],
    severity: &Severity,
    color: bool,
) -> std::fmt::Result {
    if group.is_empty() {
        return Ok(());
    }

    let first_label = group[0];

    // Collect all lines that have labels
    let mut label_lines: Vec<usize> = group.iter().map(|l| l.span.start.line).collect();
    label_lines.sort();
    label_lines.dedup();

    // Add context lines (1 above and below for each label line)
    let context_lines = 1;
    let mut lines_to_show = HashSet::new();
    let total_lines = first_label.span.source.content().lines().count();

    for &line in &label_lines {
        let start = line.saturating_sub(context_lines).max(1);
        let end = (line + context_lines).min(total_lines);
        for l in start..=end {
            lines_to_show.insert(l);
        }
    }

    let mut lines_vec: Vec<usize> = lines_to_show.into_iter().collect();
    lines_vec.sort();

    // Display lines with gaps indicated by dots
    let mut prev_line = 0;
    for &line_num in &lines_vec {
        // Show dots if there's a gap
        if prev_line > 0 && line_num > prev_line + 1 {
            writeln!(f, "{}{}", GUTTER, surface("┆"))?;
        }

        if let Some(line_text) = first_label
            .span
            .source
            .content()
            .lines()
            .nth(line_num.saturating_sub(1))
        {
            // Determine line number color based on labels
            let line_color = {
                let mut has_primary = false;
                let mut has_secondary = false;

                for label in group {
                    if label.span.start.line == line_num {
                        if label.is_primary {
                            has_primary = true;
                        } else {
                            has_secondary = true;
                        }
                    }
                }

                let sc = severity.color();
                if has_primary || has_secondary {
                    format!("{}", format!("{:>4}", line_num).truecolor(sc.0, sc.1, sc.2))
                } else {
                    subtext(format!("{:>4}", line_num))
                }
            };

            writeln!(
                f,
                "{} {} {}",
                line_color,
                surface("│"),
                syntax_highlight(line_text)
            )?;

            // Collect labels for this line
            let line_labels: Vec<&&Label> = group
                .iter()
                .filter(|label| label.span.start.line == line_num)
                .collect();

            // Draw underlines and labels beneath the code
            for label in &line_labels {
                let col_start = label.span.start.col.saturating_sub(1);
                let col_end = if label.span.start.line == label.span.end.line {
                    label.span.end.col.saturating_sub(1)
                } else {
                    line_text.len()
                };
                let width = col_end.saturating_sub(col_start).max(1);

                let sc = severity.color();
                let color_fn: Box<dyn Fn(&str) -> String> = if label.is_primary {
                    Box::new(move |s: &str| format!("{}", s.truecolor(sc.0, sc.1, sc.2).bold()))
                } else {
                    Box::new(move |s: &str| format!("{}", s.truecolor(sc.0, sc.1, sc.2)))
                };

                let c = if label.is_primary { '━' } else { '─' };
                let underline_text = format!(
                    "{}{} {}",
                    " ".repeat(col_start),
                    color_fn(&c.to_string().repeat(width)),
                    label.message.render(color)
                );
                writeln!(f, "{}{} {}", GUTTER, surface("│"), underline_text)?;
            }
        }

        prev_line = line_num;
    }

    // Show file location at the bottom right
    let first_primary = group.iter().find(|l| l.is_primary).unwrap_or(&first_label);
    let location = format!(
        "{}:{}:{}",
        first_primary.span.source.identifier(),
        first_primary.span.start.line,
        first_primary.span.start.col,
    );

    let dash_count = 65usize.saturating_sub(location.len());
    writeln!(
        f,
        "{}{}{}",
        GUTTER,
        surface(" ".repeat(dash_count)),
        subtext(&location),
    )?;

    Ok(())
}

/// Group labels by source file, returning groups of labels from the same source.
fn group_labels_by_source<'a>(labels: &'a [Label]) -> Vec<Vec<&'a Label>> {
    let mut groups: Vec<Vec<&'a Label>> = Vec::new();

    for label in labels {
        let mut added = false;
        for group in &mut groups {
            if let Some(first) = group.first() {
                let same_source = std::ptr::eq(
                    first.span.source.as_ref() as *const _,
                    label.span.source.as_ref() as *const _,
                );

                if same_source {
                    group.push(label);
                    added = true;
                    break;
                }
            }
        }

        if !added {
            groups.push(vec![label]);
        }
    }

    groups
}

/// Plain text; use [`Diagnostic::render`] for colour.
impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render(false))
    }
}

struct Rendered<'a> {
    diagnostic: &'a Diagnostic,
    color: bool,
}

impl Display for Rendered<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.diagnostic;
        writeln!(f, "{}: {}", d.severity, d.message.render(self.color))?;

        // Code snippets get some room; a diagnostic without them stays compact.
        if !d.labels.is_empty() {
            writeln!(f)?;
            for group in group_labels_by_source(&d.labels) {
                fmt_label_group(f, &group, &d.severity, self.color)?;
            }
            if !d.hints.is_empty() || !d.trace.is_empty() {
                writeln!(f)?;
            }
        }

        for hint in &d.hints {
            writeln!(f, "{}{} {}", GUTTER, teal("?"), hint.render(self.color))?;
        }

        for trace in &d.trace {
            writeln!(f, "{}{} {}", GUTTER, subtext("!"), trace)?;
        }

        Ok(())
    }
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

#[derive(Debug, Clone)]
pub enum Severity {
    Error,
    Warning,
    Note,
    /// The happy path, labelled with what happened, like `wrote` or `built`.
    Success(&'static str),
}

impl Severity {
    fn color(&self) -> Rgb {
        match self {
            Severity::Error => RED,
            Severity::Warning => YELLOW,
            Severity::Note => BLUE,
            Severity::Success(_) => GREEN,
        }
    }
}

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = self.color();
        write!(
            f,
            "{}",
            match self {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
                Severity::Success(what) => what,
            }
            .truecolor(c.0, c.1, c.2)
            .bold()
        )
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub message: Message,
    pub span: Span,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<Message>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: true,
        }
    }

    pub fn secondary(span: Span, message: impl Into<Message>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: false,
        }
    }
}

impl Span {
    pub fn label(&self, message: impl Into<Message>) -> Label {
        Label {
            message: message.into(),
            span: self.clone(),
            is_primary: true,
        }
    }
}

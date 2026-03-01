use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use owo_colors::{OwoColorize, Rgb};

use crate::Span;
use crate::lexer::{TokenKind, lex};
use crate::span::Source;

// Catppuccin Mocha colors
#[allow(dead_code)]
const ROSEWATER: Rgb = Rgb(245, 224, 220);
#[allow(dead_code)]
const FLAMINGO: Rgb = Rgb(242, 205, 205);
#[allow(dead_code)]
const PINK: Rgb = Rgb(245, 194, 231);
const MAUVE: Rgb = Rgb(203, 166, 247);
const RED: Rgb = Rgb(243, 139, 168);
#[allow(dead_code)]
const MAROON: Rgb = Rgb(235, 160, 172);
const PEACH: Rgb = Rgb(250, 179, 135);
const YELLOW: Rgb = Rgb(249, 226, 175);
#[allow(dead_code)]
const GREEN: Rgb = Rgb(166, 227, 161);
const TEAL: Rgb = Rgb(148, 226, 213);
const SKY: Rgb = Rgb(137, 220, 235);
#[allow(dead_code)]
const SAPPHIRE: Rgb = Rgb(116, 199, 236);
const BLUE: Rgb = Rgb(137, 180, 250);
#[allow(dead_code)]
const LAVENDER: Rgb = Rgb(180, 190, 254);
const TEXT: Rgb = Rgb(205, 214, 244);
#[allow(dead_code)]
const SUBTEXT1: Rgb = Rgb(186, 194, 222);
const SUBTEXT0: Rgb = Rgb(166, 173, 200);
#[allow(dead_code)]
const OVERLAY2: Rgb = Rgb(147, 153, 178);
#[allow(dead_code)]
const OVERLAY1: Rgb = Rgb(127, 132, 156);
#[allow(dead_code)]
const OVERLAY0: Rgb = Rgb(108, 112, 134);
const SURFACE2: Rgb = Rgb(88, 91, 112);
#[allow(dead_code)]
const SURFACE1: Rgb = Rgb(69, 71, 90);
#[allow(dead_code)]
const SURFACE0: Rgb = Rgb(49, 50, 68);
#[allow(dead_code)]
const BASE: Rgb = Rgb(30, 30, 46);
#[allow(dead_code)]
const MANTLE: Rgb = Rgb(24, 24, 37);
#[allow(dead_code)]
const CRUST: Rgb = Rgb(17, 17, 27);

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

/// Levenshtein edit distance between two strings.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i;
    }
    for j in 0..=b_len {
        matrix[0][j] = j;
    }

    for (i, ca) in a.chars().enumerate() {
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            matrix[i + 1][j + 1] = (matrix[i][j + 1] + 1)
                .min(matrix[i + 1][j] + 1)
                .min(matrix[i][j] + cost);
        }
    }

    matrix[a_len][b_len]
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

pub trait Highlight: Display {
    fn as_type(&self) -> String {
        format!(
            "{}",
            self.to_string().truecolor(YELLOW.0, YELLOW.1, YELLOW.2)
        )
    }
    fn as_var(&self) -> String {
        format!(
            "{}",
            self.to_string().truecolor(TEXT.0, TEXT.1, TEXT.2).bold()
        )
    }
    fn as_func(&self) -> String {
        format!("{}", self.to_string().truecolor(BLUE.0, BLUE.1, BLUE.2))
    }
    fn as_struct(&self) -> String {
        format!("{}", self.to_string().truecolor(PEACH.0, PEACH.1, PEACH.2))
    }
    fn as_field(&self) -> String {
        format!("{}", self.to_string().truecolor(TEAL.0, TEAL.1, TEAL.2))
    }
    fn as_keyword(&self) -> String {
        format!(
            "{}",
            self.to_string()
                .truecolor(MAUVE.0, MAUVE.1, MAUVE.2)
                .italic()
        )
    }
    fn as_module(&self) -> String {
        format!("{}", self.to_string().truecolor(GREEN.0, GREEN.1, GREEN.2))
    }
}

impl<T: Display> Highlight for T {}

// Map token kinds to colors
fn token_color(kind: TokenKind) -> Rgb {
    match kind {
        TokenKind::Fn
        | TokenKind::Extern
        | TokenKind::Let
        | TokenKind::If
        | TokenKind::Else
        | TokenKind::Loop
        | TokenKind::While
        | TokenKind::For
        | TokenKind::Struct
        | TokenKind::Use
        | TokenKind::Mut
        | TokenKind::Impl => MAUVE,
        TokenKind::I8
        | TokenKind::I16
        | TokenKind::I32
        | TokenKind::I64
        | TokenKind::I128
        | TokenKind::ISize
        | TokenKind::U8
        | TokenKind::U16
        | TokenKind::U32
        | TokenKind::U64
        | TokenKind::U128
        | TokenKind::USize
        | TokenKind::F32
        | TokenKind::F64
        | TokenKind::Bool
        | TokenKind::Char
        | TokenKind::Str => YELLOW,
        TokenKind::Int
        | TokenKind::Float
        | TokenKind::Text
        | TokenKind::True
        | TokenKind::False => PEACH,
        TokenKind::Ident => TEXT,
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Ampersand
        | TokenKind::Slash
        | TokenKind::Equals
        | TokenKind::DoubleEquals
        | TokenKind::NotEquals
        | TokenKind::Bang
        | TokenKind::LessThan
        | TokenKind::GreaterThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThanOrEqual
        | TokenKind::Percentage => SKY,
        TokenKind::OpenParen
        | TokenKind::CloseParen
        | TokenKind::OpenBrace
        | TokenKind::CloseBrace
        | TokenKind::Comma
        | TokenKind::Colon
        | TokenKind::Semicolon
        | TokenKind::DoubleColon
        | TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::SingleQuote
        | TokenKind::DoubleQuote
        | TokenKind::Dot => SUBTEXT0,
        TokenKind::Comment => SURFACE2,
        TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error => TEXT,
    }
}

// Check if a token should be italic
fn is_italic(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn | TokenKind::Let | TokenKind::If | TokenKind::Else | TokenKind::Mut | TokenKind::Extern | // Keywords
        TokenKind::Comment // Comments
    )
}

// Syntax highlight a line of source code
fn syntax_highlight(line: &str) -> String {
    if line.trim().is_empty() {
        return line.to_string();
    }

    let source = Arc::new(Source::from_raw(line));
    let tokens = lex(source);

    let mut result = String::new();

    for token in tokens {
        if token.kind == TokenKind::Eof {
            break;
        }

        let color = token_color(token.kind);
        let colored_text = if is_italic(token.kind) {
            format!(
                "{}",
                token.text.truecolor(color.0, color.1, color.2).italic()
            )
        } else {
            format!("{}", token.text.truecolor(color.0, color.1, color.2))
        };
        result.push_str(&colored_text);
    }

    result
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub trace: Vec<String>,
    pub message: String,
    pub hints: Vec<String>,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            trace: vec![],
            message: message.into(),
            hints: vec![],
            labels: vec![],
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    pub fn note(message: impl Into<String>) -> Self {
        Self::new(Severity::Note, message)
    }

    pub fn with_label(mut self, label: impl Into<Label>) -> Self {
        self.labels.push(label.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
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
}

/// The gutter prefix: 4 chars for line number + space + pipe + space = "     │ "
const GUTTER: &str = "     ";

/// Render a group of labels from the same source file.
fn fmt_label_group(
    f: &mut std::fmt::Formatter<'_>,
    group: &[&Label],
    severity: &Severity,
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
                if has_primary {
                    format!("{}", format!("{:>4}", line_num).truecolor(sc.0, sc.1, sc.2))
                } else if has_secondary {
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
                    &label.message
                );
                writeln!(f, "{}{} {}", GUTTER, surface("│"), underline_text)?;
            }
        }

        prev_line = line_num;
    }

    // Show file location at the bottom right
    let first_primary = group.iter().find(|l| l.is_primary).unwrap_or(&&first_label);
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
        surface(&" ".repeat(dash_count)),
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

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.severity, &self.message)?;
        writeln!(f)?;

        for group in group_labels_by_source(&self.labels) {
            fmt_label_group(f, &group, &self.severity)?;
        }

        if !self.hints.is_empty() || !self.trace.is_empty() {
            writeln!(f)?;
        }

        for hint in &self.hints {
            writeln!(f, "{}{} {}", GUTTER, teal("?"), hint)?;
        }

        for trace in &self.trace {
            writeln!(f, "{}{} {}", GUTTER, subtext("!"), trace)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    fn color(&self) -> Rgb {
        match self {
            Severity::Error => RED,
            Severity::Warning => YELLOW,
            Severity::Note => BLUE,
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
            }
            .truecolor(c.0, c.1, c.2)
            .bold()
        )
    }
}

#[derive(Debug, Clone)]
pub struct Label {
    pub message: String,
    pub span: Span,
    pub is_primary: bool,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: true,
        }
    }

    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Label {
            message: message.into(),
            span,
            is_primary: false,
        }
    }
}

impl Span {
    pub fn label(&self, message: impl Into<String>) -> Label {
        Label {
            message: message.into(),
            span: self.clone(),
            is_primary: true,
        }
    }
}

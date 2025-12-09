use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

use owo_colors::{OwoColorize, Rgb};

use crate::lexer::{lex, TokenKind};
use crate::span::Source;
use crate::Span;

// Get terminal width, defaulting to 100 if unavailable
fn get_terminal_width() -> usize {
    terminal_size::terminal_size()
        .map(|(terminal_size::Width(w), _)| w as usize)
        .unwrap_or(100)
}

// Strip ANSI escape codes to get visible length
fn visible_len(s: &str) -> usize {
    let mut result = 0;
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip until we find 'm' (end of ANSI sequence)
            while let Some(c) = chars.next() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            result += 1;
        }
    }
    result
}

// Wrap text to fit within a given width, preserving ANSI codes
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_visible = 0;
    let mut chars = text.chars().peekable();
    let mut last_ansi = String::new();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Capture ANSI escape sequence
            let mut ansi = String::from('\x1b');
            while let Some(&c) = chars.peek() {
                ansi.push(c);
                chars.next();
                if c == 'm' {
                    break;
                }
            }
            current_line.push_str(&ansi);
            last_ansi = ansi;
        } else if ch == ' ' && current_visible >= max_width {
            // Word break at space when we're at limit
            lines.push(current_line);
            current_line = last_ansi.clone();
            current_visible = 0;
        } else {
            current_line.push(ch);
            current_visible += 1;

            if current_visible >= max_width && ch != ' ' {
                // Hard break in middle of word
                lines.push(current_line);
                current_line = last_ansi.clone();
                current_visible = 0;
            }
        }
    }

    if !current_line.is_empty() && visible_len(&current_line) > 0 {
        lines.push(current_line);
    }

    if lines.is_empty() {
        vec![text.to_string()]
    } else {
        lines
    }
}

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
fn red(text: impl std::fmt::Display) -> String {
    format!("{}", text.to_string().truecolor(RED.0, RED.1, RED.2))
}

fn blue(text: impl std::fmt::Display) -> String {
    format!("{}", text.to_string().truecolor(BLUE.0, BLUE.1, BLUE.2))
}

fn yellow(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string().truecolor(YELLOW.0, YELLOW.1, YELLOW.2)
    )
}

fn teal(text: impl std::fmt::Display) -> String {
    format!("{}", text.to_string().truecolor(TEAL.0, TEAL.1, TEAL.2))
}

fn text(text_str: impl std::fmt::Display) -> String {
    format!("{}", text_str.to_string().truecolor(TEXT.0, TEXT.1, TEXT.2))
}

fn subtext(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SUBTEXT0.0, SUBTEXT0.1, SUBTEXT0.2)
    )
}

fn surface(text: impl std::fmt::Display) -> String {
    format!(
        "{}",
        text.to_string()
            .truecolor(SURFACE2.0, SURFACE2.1, SURFACE2.2)
    )
}

// Map token kinds to colors
fn token_color(kind: TokenKind) -> Rgb {
    match kind {
        // Keywords: mauve
        TokenKind::Fn | TokenKind::Let | TokenKind::If | TokenKind::Else => MAUVE,

        // Built-in types: yellow
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

        // Literals: peach
        TokenKind::Int => PEACH,

        // Identifiers: text
        TokenKind::Ident => TEXT,

        // Operators: sky
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Equals
        | TokenKind::DoubleEquals
        | TokenKind::NotEquals
        | TokenKind::LessThan
        | TokenKind::GreaterThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThanOnEqual => SKY,

        // Delimiters: text
        TokenKind::OpenParen
        | TokenKind::CloseParen
        | TokenKind::OpenBrace
        | TokenKind::CloseBrace
        | TokenKind::Comma
        | TokenKind::Colon
        | TokenKind::Semicolon
        | TokenKind::Arrow
        | TokenKind::FatArrow => SUBTEXT0,

        // Comments: surface2
        TokenKind::Comment => SURFACE2,

        // Default: text
        TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error => TEXT,
    }
}

// Check if a token should be italic
fn is_italic(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Fn | TokenKind::Let | TokenKind::If | TokenKind::Else | // Keywords
        TokenKind::Comment // Comments
    )
}

// Syntax highlight a line of source code with optional error highlighting
fn syntax_highlight_with_highlight(line: &str, highlight_start: Option<usize>, highlight_end: Option<usize>, is_primary: bool) -> String {
    if line.trim().is_empty() {
        return line.to_string();
    }

    let source = Arc::new(Source::from_raw(line));
    let tokens = lex(source);

    let mut result = String::new();
    let mut current_pos = 0;

    for token in tokens {
        if token.kind == TokenKind::Eof {
            break;
        }

        let color = token_color(token.kind);
        let token_len = token.text.len();
        let token_end = current_pos + token_len;

        if let (Some(hl_start), Some(hl_end)) = (highlight_start, highlight_end) {
            // Calculate overlap between token and highlight range
            let overlap_start = hl_start.max(current_pos);
            let overlap_end = hl_end.min(token_end);

            if overlap_start < overlap_end {
                // Token has some highlighted portion
                let before_len = overlap_start.saturating_sub(current_pos);
                let highlight_len = overlap_end - overlap_start;
                let after_len = token_end.saturating_sub(overlap_end);

                // Part before highlight
                if before_len > 0 {
                    let before_text = &token.text[..before_len];
                    let colored = if is_italic(token.kind) {
                        format!("{}", before_text.truecolor(color.0, color.1, color.2).italic())
                    } else {
                        format!("{}", before_text.truecolor(color.0, color.1, color.2))
                    };
                    result.push_str(&colored);
                }

                // Highlighted part
                if highlight_len > 0 {
                    let hl_text = &token.text[before_len..before_len + highlight_len];
                    let bg_color = if is_primary { RED } else { BLUE };
                    result.push_str(&format!("{}",
                        hl_text.truecolor(0, 0, 0).on_truecolor(bg_color.0, bg_color.1, bg_color.2)
                    ));
                }

                // Part after highlight
                if after_len > 0 {
                    let after_text = &token.text[before_len + highlight_len..];
                    let colored = if is_italic(token.kind) {
                        format!("{}", after_text.truecolor(color.0, color.1, color.2).italic())
                    } else {
                        format!("{}", after_text.truecolor(color.0, color.1, color.2))
                    };
                    result.push_str(&colored);
                }
            } else {
                // No overlap - normal rendering
                let colored_text = if is_italic(token.kind) {
                    format!("{}", token.text.truecolor(color.0, color.1, color.2).italic())
                } else {
                    format!("{}", token.text.truecolor(color.0, color.1, color.2))
                };
                result.push_str(&colored_text);
            }
        } else {
            // No highlight range - normal rendering
            let colored_text = if is_italic(token.kind) {
                format!("{}", token.text.truecolor(color.0, color.1, color.2).italic())
            } else {
                format!("{}", token.text.truecolor(color.0, color.1, color.2))
            };
            result.push_str(&colored_text);
        }

        current_pos += token_len;
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
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
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

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.severity, text(&self.message))?;

        // Group labels by source
        let mut label_groups: Vec<Vec<&Label>> = Vec::new();

        for label in &self.labels {
            // Try to find a group this label belongs to (same source)
            let mut added = false;
            for group in &mut label_groups {
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
                label_groups.push(vec![label]);
            }
        }

        // Display each group
        for group in label_groups {
            if group.is_empty() {
                continue;
            }

            // Get the range of lines to display
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

            // Calculate the maximum visible line length in this group
            let max_line_len = lines_vec
                .iter()
                .filter_map(|&line_num| {
                    first_label
                        .span
                        .source
                        .content()
                        .lines()
                        .nth(line_num.saturating_sub(1))
                        .map(|line| line.len())
                })
                .max()
                .unwrap_or(0);

            // Get terminal width and calculate label column
            let term_width = get_terminal_width();
            let prefix_width = 11; // "     N │ " is approximately 11 chars
            let available_width = term_width.saturating_sub(prefix_width);
            let label_padding = 2; // Minimum padding between code and label
            let label_column = (max_line_len + label_padding).min(available_width / 2);

            // Header with location
            writeln!(
                f,
                "  {} {}:{}:{}",
                blue("-->"),
                first_label.span.source.identifier(),
                first_label.span.start.line,
                first_label.span.start.col
            )?;

            writeln!(f, "     {}", surface("│"))?;

            // Display lines with gaps indicated by dots
            let mut prev_line = 0;
            for &line_num in &lines_vec {
                // Show dots if there's a gap
                if prev_line > 0 && line_num > prev_line + 1 {
                    writeln!(f, "     {}", surface("┆"))?;
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

                        for label in &group {
                            if label.span.start.line == line_num {
                                if label.is_primary {
                                    has_primary = true;
                                } else {
                                    has_secondary = true;
                                }
                            }
                        }

                        if has_primary {
                            red(format!("{:>4}", line_num))
                        } else if has_secondary {
                            blue(format!("{:>4}", line_num))
                        } else {
                            subtext(format!("{:>4}", line_num))
                        }
                    };

                    write!(
                        f,
                        "{} {} ",
                        line_color,
                        surface("│")
                    )?;

                    // Check if this line has any labels to highlight
                    let mut highlight_start = None;
                    let mut highlight_end = None;
                    let mut is_primary = false;
                    for label in &group {
                        if label.span.start.line == line_num {
                            let col_start = label.span.start.col.saturating_sub(1);
                            let col_end = if label.span.start.line == label.span.end.line {
                                label.span.end.col.saturating_sub(1)
                            } else {
                                line_text.len()
                            };
                            // Add padding: 1 character on each side
                            highlight_start = Some(col_start.saturating_sub(1));
                            highlight_end = Some((col_end + 1).min(line_text.len()));
                            is_primary = label.is_primary;
                            break;
                        }
                    }

                    write!(f, "{}", syntax_highlight_with_highlight(line_text, highlight_start, highlight_end, is_primary))?;

                    // Track visible length for alignment
                    let mut visible_len = line_text.len();

                    // Add trailing padding space if needed
                    if let (Some(_), Some(end)) = (highlight_start, highlight_end) {
                        if end >= line_text.len() {
                            // Need to add explicit trailing space with background
                            let bg_color = if is_primary { RED } else { BLUE };
                            write!(f, "{}", " ".truecolor(0, 0, 0).on_truecolor(bg_color.0, bg_color.1, bg_color.2))?;
                            visible_len += 1;
                        }
                    }

                    // Collect labels for this line
                    let line_labels: Vec<&Label> = group
                        .iter()
                        .filter(|label| label.span.start.line == line_num)
                        .copied()
                        .collect();

                    if !line_labels.is_empty() {
                        // Align labels at the calculated column
                        let padding_needed = label_column.saturating_sub(visible_len);
                        if padding_needed > 0 {
                            write!(f, "{}", " ".repeat(padding_needed))?;
                        }

                        // Combine all labels for this line
                        let combined_label = line_labels
                            .iter()
                            .map(|label| {
                                if label.is_primary {
                                    red(&label.message).bold().to_string()
                                } else {
                                    blue(&label.message).to_string()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(", ");

                        // Calculate available width for the label
                        let current_pos = prefix_width + visible_len + padding_needed;
                        let available_for_label = term_width.saturating_sub(current_pos + 2);

                        // Wrap the label if needed
                        let wrapped_lines = wrap_text(&combined_label, available_for_label);

                        // Write first line
                        if let Some(first) = wrapped_lines.first() {
                            write!(f, "  {}", first)?;
                        }

                        // Write continuation lines with proper indentation
                        for continuation in wrapped_lines.iter().skip(1) {
                            writeln!(f)?;
                            write!(
                                f,
                                "{}{}",
                                " ".repeat(prefix_width + label_column + 2),
                                continuation
                            )?;
                        }
                    }

                    writeln!(f)?;
                }

                prev_line = line_num;
            }

            writeln!(f, "     {}", surface("│"))?;
        }

        for hint in &self.hints {
            writeln!(f, "{} {}", teal("= hint:"), text(hint))?;
        }

        for trace in &self.trace {
            writeln!(f, "{} {}", subtext("= caused by:"), text(trace))?;
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

impl Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "{}", red("error").bold()),
            Severity::Warning => write!(f, "{}", yellow("warning").bold()),
            Severity::Note => write!(f, "{}", blue("note").bold()),
        }
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

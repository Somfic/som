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
        TokenKind::Fn | TokenKind::Let | TokenKind::If | TokenKind::Else | TokenKind::Mut => MAUVE,

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
        TokenKind::Int | TokenKind::Text | TokenKind::True | TokenKind::False => PEACH,

        // Identifiers: text
        TokenKind::Ident => TEXT,

        // Operators: sky
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Ampersand
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
        | TokenKind::FatArrow
        | TokenKind::SingleQuote
        | TokenKind::DoubleQuote => SUBTEXT0,

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

                    write!(f, "{} {} ", line_color, surface("│"))?;

                    write!(f, "{}", syntax_highlight(line_text))?;

                    writeln!(f)?;

                    // Collect labels for this line
                    let line_labels: Vec<&Label> = group
                        .iter()
                        .filter(|label| label.span.start.line == line_num)
                        .copied()
                        .collect();

                    // Draw underlines and labels beneath the code
                    if !line_labels.is_empty() {
                        for label in &line_labels {
                            let col_start = label.span.start.col.saturating_sub(1);
                            let col_end = if label.span.start.line == label.span.end.line {
                                label.span.end.col.saturating_sub(1)
                            } else {
                                line_text.len()
                            };
                            let width = col_end.saturating_sub(col_start).max(1);

                            let color_fn: Box<dyn Fn(&str) -> String> = if label.is_primary {
                                Box::new(|s: &str| red(s).bold().to_string())
                            } else {
                                Box::new(|s: &str| blue(s).to_string())
                            };

                            // Draw the connecting line with label
                            write!(f, "     {} ", surface("│"))?;
                            write!(f, "{}", " ".repeat(col_start))?;

                            let c = if label.is_primary { '━' } else { '─' };
                            write!(f, "{}", color_fn(&c.to_string().repeat(width)))?;

                            write!(f, " {}", color_fn(&label.message))?;
                            writeln!(f)?;
                        }
                    }
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

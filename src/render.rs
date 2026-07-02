//! Terminal rendering of `Diagnostic`s: a source snippet with the offending
//! spans highlighted (background fill), syntax-highlighted code, and a location
//! footer.
//!
//! The diagnostic *data* (spans, labels, suggestions) lives in `som_common`;
//! this module turns it into an ANSI string. It lives in the root crate so it
//! can borrow the lexer from `som_ast` for syntax highlighting without adding a
//! dependency cycle to `som_common`.

use std::collections::HashSet;
use std::fmt::Write;

use owo_colors::{OwoColorize, Rgb, Style};
use som_ast::{TokenKind, lex};
use som_common::{
    Diagnostic, Id, Message, MessagePart, Position, Severity, Source, SourceMap, Span, Suggestion,
};

// Catppuccin Mocha palette.
const MAUVE: Rgb = Rgb(203, 166, 247);
const RED: Rgb = Rgb(243, 139, 168);
const PEACH: Rgb = Rgb(250, 179, 135);
const YELLOW: Rgb = Rgb(249, 226, 175);
const TEAL: Rgb = Rgb(148, 226, 213);
const SKY: Rgb = Rgb(137, 220, 235);
const BLUE: Rgb = Rgb(137, 180, 250);
const TEXT: Rgb = Rgb(205, 214, 244);
const SUBTEXT0: Rgb = Rgb(166, 173, 200);
const SURFACE2: Rgb = Rgb(88, 91, 112);
/// The theme's editor background — highlight backgrounds are blended toward
/// this so they read as a tint, not a solid block.
const BASE: Rgb = Rgb(30, 30, 46);

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let mix = |x: u8, y: u8| (x as f32 * (1.0 - t) + y as f32 * t).round() as u8;
    Rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Background fill for a primary span: a dark, desaturated severity tint. Paired
/// with a forced light foreground so the code stays readable over it.
fn primary_bg(sev: Rgb) -> Rgb {
    lerp(sev, BASE, 0.68)
}

/// Background fill for a secondary span — subtler than the primary.
fn secondary_bg(sev: Rgb) -> Rgb {
    lerp(sev, BASE, 0.82)
}

fn paint(text: impl std::fmt::Display, c: Rgb) -> String {
    format!("{}", text.to_string().truecolor(c.0, c.1, c.2))
}

fn teal(text: impl std::fmt::Display) -> String {
    paint(text, TEAL)
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
    paint(text, SURFACE2)
}

fn severity_color(severity: Severity) -> Rgb {
    match severity {
        Severity::Error => RED,
        Severity::Warning => YELLOW,
        Severity::Note => BLUE,
        Severity::Help => TEAL,
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Note => "note",
        Severity::Help => "help",
    }
}

fn token_color(kind: TokenKind) -> Rgb {
    use TokenKind::*;
    match kind {
        Fn | Extern | Struct | Impl | Let | If | Else | Mut | Use | Loop | While | For => MAUVE,
        I8 | I16 | I32 | I64 | I128 | ISize | U8 | U16 | U32 | U64 | U128 | USize | F32 | F64
        | Bool | Char | Str => YELLOW,
        Int | Float | Text | True | False => PEACH,
        Ident => TEXT,
        Plus | Minus | Star | Slash | Equals | DoubleEquals | NotEquals | Or | And | Bang
        | LessThan | GreaterThan | LessThanOrEquals | GreaterThanOrEquals | Percentage => SKY,
        OpenParen | CloseParen | OpenBrace | CloseBrace | Comma | Colon | Semicolon
        | DoubleColon | Arrow | FatArrow | Ampersand | Dot | SingleQuote | DoubleQuote => SUBTEXT0,
        Comment => SURFACE2,
        Whitespace | Eof | Error => TEXT,
    }
}

fn is_italic(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Fn | Let | If | Else | Mut | Extern | Use | Struct | Impl | Loop | While | For | Comment
    )
}

/// A highlighted span within a line, in byte columns (0-indexed, `[start, end)`).
struct Hl {
    start: usize,
    end: usize,
    primary: bool,
}

/// The visual style of a single cell: foreground, optional background, and
/// attributes. Kept as plain `Copy` scalars so adjacent equal cells can be
/// coalesced into one styled run.
#[derive(PartialEq, Clone, Copy)]
struct Cell {
    fg: (u8, u8, u8),
    bg: Option<(u8, u8, u8)>,
    bold: bool,
    italic: bool,
}

/// Render one source line: syntax colors as foreground, with the highlighted
/// spans painted as a background tint. Inside a highlight the syntax color is
/// dropped in favor of a forced light foreground, so contrast is guaranteed
/// regardless of the token underneath.
fn styled_line(line: &str, highlights: &[Hl], sev: Rgb) -> String {
    // Extend the canvas one past line end so a zero-width span (e.g. EOF) still
    // shows as a highlighted cell.
    let max_end = highlights.iter().map(|h| h.end).max().unwrap_or(0);
    let width = line.len().max(max_end);

    // Per-byte syntax foreground, defaulting to plain text.
    let mut fg = vec![(TEXT.0, TEXT.1, TEXT.2); width];
    let mut italic = vec![false; width];
    if !line.trim().is_empty() {
        for token in lex(Span::DUMMY.source, line) {
            if token.kind == TokenKind::Eof {
                continue;
            }
            let c = token_color(token.kind);
            let it = is_italic(token.kind);
            let end = (token.span.end as usize).min(line.len());
            for b in token.span.start as usize..end {
                fg[b] = (c.0, c.1, c.2);
                italic[b] = it;
            }
        }
    }

    let cell_at = |i: usize| -> Cell {
        // Primary highlight wins over secondary if they overlap.
        let mut bg = None;
        let mut primary = false;
        for h in highlights {
            if i >= h.start && i < h.end {
                if h.primary {
                    let c = primary_bg(sev);
                    return Cell {
                        fg: (TEXT.0, TEXT.1, TEXT.2),
                        bg: Some((c.0, c.1, c.2)),
                        bold: true,
                        italic: false,
                    };
                }
                let c = secondary_bg(sev);
                bg = Some((c.0, c.1, c.2));
                primary = false;
            }
        }
        match bg {
            Some(bg) => Cell {
                fg: (TEXT.0, TEXT.1, TEXT.2),
                bg: Some(bg),
                bold: primary,
                italic: false,
            },
            None => Cell {
                fg: fg[i],
                bg: None,
                bold: false,
                italic: italic[i],
            },
        }
    };

    // Walk cells, coalescing runs of identical style. Padding cells beyond the
    // real line are rendered as spaces.
    let ch_at = |i: usize| -> char { line[i..].chars().next().unwrap_or(' ') };

    let mut out = String::new();
    let mut run = String::new();
    let mut run_cell: Option<Cell> = None;
    let mut i = 0;
    while i < width {
        let cell = cell_at(i);
        if run_cell != Some(cell) {
            flush_run(&mut out, &run, run_cell);
            run.clear();
            run_cell = Some(cell);
        }
        let ch = ch_at(i);
        run.push(ch);
        i += ch.len_utf8();
    }
    flush_run(&mut out, &run, run_cell);
    out
}

fn flush_run(out: &mut String, run: &str, cell: Option<Cell>) {
    if run.is_empty() {
        return;
    }
    let Some(cell) = cell else {
        out.push_str(run);
        return;
    };
    let mut style = Style::new().truecolor(cell.fg.0, cell.fg.1, cell.fg.2);
    if let Some(bg) = cell.bg {
        style = style.on_truecolor(bg.0, bg.1, bg.2);
    }
    if cell.bold {
        style = style.bold();
    }
    if cell.italic {
        style = style.italic();
    }
    let _ = write!(out, "{}", run.style(style));
}

/// A label resolved against its source: byte spans turned into line/columns.
struct Resolved<'a> {
    message: &'a str,
    is_primary: bool,
    start: Position,
    end: Position,
}

/// The gutter under a numbered line: 4 cols of line number + 1 space, so the
/// `│` separator lines up with `NNNN │`.
const GUTTER: &str = "     ";

/// Render a structured message: prose verbatim, code fragments syntax-highlighted.
fn render_message(msg: &Message) -> String {
    let mut out = String::new();
    for part in &msg.parts {
        match part {
            MessagePart::Text(t) => out.push_str(t),
            MessagePart::Code(c) => out.push_str(&highlight_code(c)),
        }
    }
    out
}

/// Syntax-highlight a short code fragment (no background), reusing the line
/// styler with no highlighted spans.
fn highlight_code(code: &str) -> String {
    styled_line(code, &[], TEXT)
}

/// The terminal's column count, so the location can align to the real right
/// edge. Falls back to 80 when output isn't a terminal (e.g. piped to a file).
fn terminal_width() -> usize {
    terminal_size::terminal_size_of(std::io::stderr())
        .map(|(w, _)| w.0 as usize)
        .unwrap_or(80)
}

pub fn render_diagnostic(diag: &Diagnostic, sources: &SourceMap) -> String {
    let mut out = String::new();
    let sc = severity_color(diag.severity);

    // Header: `severity: message`, with the primary span's location tucked to
    // the right edge on the same line.
    let label = severity_label(diag.severity);
    let src = sources.source(diag.primary.span.source);
    let pos = src.position(diag.primary.span.start);
    let location = format!("{}:{}:{}", src.identifier(), pos.line, pos.col);

    // Pad from the visible (un-colored) width so ANSI codes don't skew it.
    let header_len = label.chars().count() + 2 + diag.message.plain().chars().count();
    let pad = terminal_width()
        .saturating_sub(header_len + location.chars().count())
        .max(1);
    let _ = writeln!(
        out,
        "{}: {}{}{}",
        label.truecolor(sc.0, sc.1, sc.2).bold(),
        render_message(&diag.message),
        " ".repeat(pad),
        subtext(&location),
    );
    let _ = writeln!(out);

    // Flatten primary + secondary into one list, then group by source file.
    let mut items: Vec<(Span, &str, bool)> = vec![(diag.primary.span, &diag.primary.message, true)];
    for label in &diag.secondary {
        items.push((label.span, &label.message, false));
    }

    let mut by_source: Vec<(Id<Source>, Vec<Resolved>)> = Vec::new();
    for (span, message, is_primary) in items {
        let src = sources.source(span.source);
        let resolved = Resolved {
            message,
            is_primary,
            start: src.position(span.start),
            end: src.position(span.end),
        };
        match by_source.iter_mut().find(|(id, _)| *id == span.source) {
            Some((_, group)) => group.push(resolved),
            None => by_source.push((span.source, vec![resolved])),
        }
    }

    for (source_id, group) in &by_source {
        render_group(&mut out, sources.source(*source_id), group, diag.severity);
    }

    if !diag.suggestions.is_empty() || !diag.notes.is_empty() {
        let _ = writeln!(out);
    }
    for suggestion in &diag.suggestions {
        render_suggestion(&mut out, suggestion);
    }
    for note in &diag.notes {
        let _ = writeln!(out, "{}{} {}", GUTTER, subtext("!"), note);
    }

    out
}

fn render_suggestion(out: &mut String, suggestion: &Suggestion) {
    if suggestion.replacement.is_empty() {
        let _ = writeln!(out, "{}{} {}", GUTTER, teal("?"), suggestion.message);
    } else {
        let _ = writeln!(
            out,
            "{}{} {}: `{}`",
            GUTTER,
            teal("?"),
            suggestion.message,
            teal(&suggestion.replacement)
        );
    }
}

fn render_group(out: &mut String, source: &Source, group: &[Resolved], severity: Severity) {
    if group.is_empty() {
        return;
    }
    let sc = severity_color(severity);

    // Lines carrying a label, plus one line of context on either side.
    let mut label_lines: Vec<u32> = group.iter().map(|l| l.start.line).collect();
    label_lines.sort_unstable();
    label_lines.dedup();

    let total_lines = source.content().lines().count() as u32;
    let mut lines_to_show: HashSet<u32> = HashSet::new();
    for &line in &label_lines {
        let start = line.saturating_sub(1).max(1);
        let end = (line + 1).min(total_lines);
        for l in start..=end {
            lines_to_show.insert(l);
        }
    }
    let mut lines: Vec<u32> = lines_to_show.into_iter().collect();
    lines.sort_unstable();

    let mut prev_line = 0u32;
    for &line_num in &lines {
        // A break in the sequence gets an ellipsis row.
        if prev_line > 0 && line_num > prev_line + 1 {
            let _ = writeln!(out, "{}{}", GUTTER, surface("┆"));
        }

        let Some(line_text) = source.content().lines().nth((line_num - 1) as usize) else {
            continue;
        };

        let has_label = group.iter().any(|l| l.start.line == line_num);
        let number = if has_label {
            paint(format!("{line_num:>4}"), sc)
        } else {
            subtext(format!("{line_num:>4}"))
        };
        // Byte-column span for each label on this line, used both to paint the
        // background highlight and to place the message pointer beneath.
        let on_line: Vec<(usize, &Resolved)> = group
            .iter()
            .filter(|l| l.start.line == line_num)
            .map(|label| {
                let col_start = label.start.col.saturating_sub(1) as usize;
                (col_start, label)
            })
            .collect();

        let highlights: Vec<Hl> = on_line
            .iter()
            .map(|(col_start, label)| {
                let raw_end = if label.start.line == label.end.line {
                    label.end.col.saturating_sub(1) as usize
                } else {
                    line_text.len()
                };
                Hl {
                    start: *col_start,
                    end: raw_end.max(col_start + 1),
                    primary: label.is_primary,
                }
            })
            .collect();

        let _ = writeln!(
            out,
            "{} {} {}",
            number,
            surface("│"),
            styled_line(line_text, &highlights, sc)
        );

        // A pointer + message beneath each labeled span (skip empty messages —
        // the highlight already marks the location).
        for (col_start, label) in &on_line {
            if label.message.is_empty() {
                continue;
            }
            let caret = if label.is_primary {
                format!("{}", "↑".truecolor(sc.0, sc.1, sc.2).bold())
            } else {
                paint("↑", sc)
            };
            let _ = writeln!(
                out,
                "{}{} {}{} {}",
                GUTTER,
                surface("│"),
                " ".repeat(*col_start),
                caret,
                label.message
            );
        }

        prev_line = line_num;
    }
}

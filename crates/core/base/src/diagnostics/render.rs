use std::collections::BTreeSet;
use std::fmt::Display;

use owo_colors::OwoColorize;

use super::palette::*;
use super::text::strip_ansi;
use super::{Diagnostic, Fix, Label, Severity};
use crate::Span;

pub type Highlighter = fn(&str) -> String;

#[derive(Clone, Copy, Default)]
pub struct RenderOptions {
    pub color: bool,
    /// Without one, code snippets are shown uncoloured.
    pub highlight: Option<Highlighter>,
}

impl RenderOptions {
    pub(crate) fn highlight(&self, line: &str) -> String {
        match self.highlight {
            Some(highlight) if !line.trim().is_empty() => highlight(line),
            _ => line.to_string(),
        }
    }
}

impl Diagnostic {
    pub fn render(&self, color: bool) -> String {
        self.render_with(RenderOptions {
            color,
            highlight: None,
        })
    }

    pub fn render_with(&self, options: RenderOptions) -> String {
        let text = Rendered {
            diagnostic: self,
            options,
        }
        .to_string();
        if options.color {
            text
        } else {
            strip_ansi(&text)
        }
    }
}

const GUTTER: &str = "     ";
const CONTEXT_LINES: usize = 1;
const LOCATION_WIDTH: usize = 65;

fn line_number(n: usize, rgb: Rgb) -> String {
    format!("{}", format!("{n:>4}").truecolor(rgb.0, rgb.1, rgb.2))
}

/// Labels from one source file, under the lines they point at.
fn fmt_label_group(
    f: &mut std::fmt::Formatter<'_>,
    group: &[&Label],
    severity: &Severity,
    options: &RenderOptions,
) -> std::fmt::Result {
    let Some(&first_label) = group.first() else {
        return Ok(());
    };
    let content = first_label.span.source.content();
    let mut prev_line = 0;
    for line_num in lines_to_show(group, content.lines().count()) {
        if prev_line > 0 && line_num > prev_line + 1 {
            writeln!(f, "{}{}", GUTTER, surface("┆"))?;
        }
        if let Some(line_text) = content.lines().nth(line_num.saturating_sub(1)) {
            let on_line: Vec<&Label> = (group.iter().copied())
                .filter(|label| label.span.start.line == line_num)
                .collect();
            let number = match on_line.is_empty() {
                true => margin(format!("{:>4}", line_num)),
                false => line_number(line_num, severity.color()),
            };
            writeln!(
                f,
                "{} {} {}",
                number,
                surface("│"),
                options.highlight(line_text)
            )?;
            for label in on_line {
                fmt_underline(f, label, line_text, severity, options)?;
            }
        }
        prev_line = line_num;
    }

    let first_primary = group.iter().find(|l| l.is_primary).unwrap_or(&first_label);
    let location = format!(
        "{}:{}:{}",
        first_primary.span.source.identifier(),
        first_primary.span.start.line,
        first_primary.span.start.col,
    );
    writeln!(
        f,
        "{}{}{}",
        GUTTER,
        surface(" ".repeat(LOCATION_WIDTH.saturating_sub(location.len()))),
        subtext(&location),
    )
}

/// The lines the labels are on, with the lines around them, in order.
fn lines_to_show(group: &[&Label], total_lines: usize) -> BTreeSet<usize> {
    let mut lines = BTreeSet::new();
    for label in group {
        let line = label.span.start.line;
        let start = line.saturating_sub(CONTEXT_LINES).max(1);
        let end = (line + CONTEXT_LINES).min(total_lines);
        lines.extend(start..=end);
    }
    lines
}

fn fmt_underline(
    f: &mut std::fmt::Formatter<'_>,
    label: &Label,
    line_text: &str,
    severity: &Severity,
    options: &RenderOptions,
) -> std::fmt::Result {
    let col_start = label.span.start.col.saturating_sub(1);
    let col_end = if label.span.start.line == label.span.end.line {
        label.span.end.col.saturating_sub(1)
    } else {
        line_text.len()
    };
    let width = col_end.saturating_sub(col_start).max(1);
    let sc = severity.color();
    let underline = if label.is_primary {
        let line = "━".repeat(width);
        format!("{}", line.truecolor(sc.0, sc.1, sc.2).bold())
    } else {
        let line = "─".repeat(width);
        format!("{}", line.truecolor(sc.0, sc.1, sc.2))
    };
    writeln!(
        f,
        "{}{} {}{} {}",
        GUTTER,
        surface("│"),
        " ".repeat(col_start),
        underline,
        label.message.render_with(*options)
    )
}

/// A fix as a diff of the lines it changes. Each edit replaces a whole node (a name, a
/// path) or inserts whole lines, so the removed and added parts are exactly the edits.
fn fmt_fix(
    f: &mut std::fmt::Formatter<'_>,
    fix: &Fix,
    options: &RenderOptions,
) -> std::fmt::Result {
    // Edits on the same line are shown together, as the line comes out: `&(n + 1)` is one
    // change, not a `&(` and a `)`.
    let mut shown: Vec<usize> = Vec::new();
    for (i, (span, text)) in fix.edits.iter().enumerate() {
        if shown.contains(&i) {
            continue;
        }
        let content = span.source.content();
        let (start, end) = (span.start_offset, span.start_offset + span.length);
        if inserts_lines(span, text) {
            fmt_inserted_lines(f, span, text, options)?;
        } else if content[start..end].contains('\n') || text.contains('\n') {
            shown.push(i);
            fmt_multiline_edit(f, span, text, options)?;
        } else {
            fmt_line_edits(f, fix, span, &mut shown, options)?;
        }
    }
    Ok(())
}

fn line_start(content: &str, offset: usize) -> usize {
    content[..offset].rfind('\n').map_or(0, |i| i + 1)
}

fn line_end(content: &str, offset: usize) -> usize {
    content[offset..]
        .find('\n')
        .map_or(content.len(), |i| offset + i)
}

/// Whole lines put in front of a line: `extern "raylib";`.
fn inserts_lines(span: &Span, text: &str) -> bool {
    span.length == 0
        && span.start_offset == line_start(span.source.content(), span.start_offset)
        && text.ends_with('\n')
}

fn changed(text: &str, rgb: Rgb) -> String {
    format!("{}", text.truecolor(rgb.0, rgb.1, rgb.2).bold().underline())
}

fn marker(text: &str, rgb: Rgb) -> String {
    format!("{}", text.truecolor(rgb.0, rgb.1, rgb.2).bold())
}

fn fmt_added(f: &mut std::fmt::Formatter<'_>, line: usize, text: &str) -> std::fmt::Result {
    writeln!(
        f,
        "{} {} {}",
        line_number(line, GREEN),
        marker("+", GREEN),
        changed(text, GREEN)
    )
}

fn fmt_inserted_lines(
    f: &mut std::fmt::Formatter<'_>,
    span: &Span,
    text: &str,
    options: &RenderOptions,
) -> std::fmt::Result {
    let content = span.source.content();
    let line = span.start.line;
    for (i, added) in text.lines().enumerate() {
        fmt_added(f, line + i, added)?;
    }
    if let Some(next) = content[span.start_offset..].lines().next() {
        let n = line + text.lines().count();
        writeln!(
            f,
            "{} {}   {}",
            margin(format!("{n:>4}")),
            surface("│"),
            options.highlight(next)
        )?;
    }
    Ok(())
}

/// An edit across lines, like removing a declaration or replacing a block: the lines it
/// touches, then what they become.
fn fmt_multiline_edit(
    f: &mut std::fmt::Formatter<'_>,
    span: &Span,
    text: &str,
    options: &RenderOptions,
) -> std::fmt::Result {
    let content = span.source.content();
    let (start, end) = (span.start_offset, span.start_offset + span.length);
    let line_start = line_start(content, start);
    let line = span.start.line;
    // Ending just after a newline leaves the next line alone.
    let last = if end > start && content[..end].ends_with('\n') {
        end - 1
    } else {
        line_end(content, end)
    };
    let mut offset = line_start;
    for (n, removed) in content[line_start..last.max(line_start)]
        .split('\n')
        .enumerate()
    {
        let (from, to) = (offset, offset + removed.len());
        let (cut_from, cut_to) = (start.clamp(from, to), end.clamp(from, to));
        writeln!(
            f,
            "{} {} {}{}{}",
            line_number(line + n, RED),
            marker("-", RED),
            options.highlight(&content[from..cut_from]),
            changed(&content[cut_from..cut_to], RED),
            options.highlight(&content[cut_to..to])
        )?;
        offset = to + 1;
    }
    let after_end = if last < end { last } else { end };
    let added = format!(
        "{}{text}{}",
        &content[line_start..start],
        &content[after_end.max(start)..last.max(after_end)]
    );
    if !added.trim().is_empty() {
        for (n, added) in added.trim_end_matches('\n').split('\n').enumerate() {
            fmt_added(f, line + n, added)?;
        }
    }
    Ok(())
}

/// Every edit on `span`'s line not shown yet, as one removed line and one added line.
fn fmt_line_edits(
    f: &mut std::fmt::Formatter<'_>,
    fix: &Fix,
    span: &Span,
    shown: &mut Vec<usize>,
    options: &RenderOptions,
) -> std::fmt::Result {
    let content = span.source.content();
    let line = span.start.line;
    let line_start = line_start(content, span.start_offset);
    let line_end = line_end(content, span.start_offset + span.length);
    let mut on_line: Vec<(usize, usize, &str)> = Vec::new();
    for (j, (other, other_text)) in fix.edits.iter().enumerate() {
        if !shown.contains(&j)
            && other.source.identifier() == span.source.identifier()
            && other.start.line == line
            && !inserts_lines(other, other_text)
        {
            shown.push(j);
            let at = other.start_offset;
            on_line.push((at, at + other.length, other_text.as_str()));
        }
    }
    on_line.sort_by_key(|&(at, ..)| at);

    let (mut removed_line, mut added_line) = (String::new(), String::new());
    let mut at = line_start;
    for &(edit_start, edit_end, inserted) in &on_line {
        let kept = options.highlight(&content[at..edit_start]);
        removed_line.push_str(&kept);
        removed_line.push_str(&changed(&content[edit_start..edit_end], RED));
        added_line.push_str(&kept);
        added_line.push_str(&changed(inserted, GREEN));
        at = edit_end;
    }
    let rest = options.highlight(&content[at..line_end]);
    writeln!(
        f,
        "{} {} {}{}",
        line_number(line, RED),
        marker("-", RED),
        removed_line,
        rest
    )?;
    writeln!(
        f,
        "{} {} {}{}",
        line_number(line, GREEN),
        marker("+", GREEN),
        added_line,
        rest
    )
}

fn group_labels_by_source(labels: &[Label]) -> Vec<Vec<&Label>> {
    let mut groups: Vec<Vec<&Label>> = Vec::new();
    for label in labels {
        let same_source = |group: &&mut Vec<&Label>| {
            std::sync::Arc::ptr_eq(&group[0].span.source, &label.span.source)
        };
        match groups.iter_mut().find(same_source) {
            Some(group) => group.push(label),
            None => groups.push(vec![label]),
        }
    }
    groups
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.render(false))
    }
}

struct Rendered<'a> {
    diagnostic: &'a Diagnostic,
    options: RenderOptions,
}

impl Display for Rendered<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let d = self.diagnostic;
        writeln!(f, "{}: {}", d.severity, d.message.render_with(self.options))?;

        // Code snippets and hints get some room from the message; a diagnostic with neither
        // stays one line.
        if !d.labels.is_empty() || !d.hints.is_empty() || !d.trace.is_empty() {
            writeln!(f)?;
        }
        if !d.labels.is_empty() {
            for group in group_labels_by_source(&d.labels) {
                fmt_label_group(f, &group, &d.severity, &self.options)?;
            }
            if !d.hints.is_empty() || !d.trace.is_empty() {
                writeln!(f)?;
            }
        }

        for hint in &d.hints {
            let hint = styled(&hint.render_with(self.options), ITALIC, self.options.color);
            writeln!(f, "{}{} {}", GUTTER, teal("┆"), hint)?;
        }

        for fix in &d.fixes {
            writeln!(f)?;
            writeln!(
                f,
                "{}{} {}",
                GUTTER,
                teal("▪"),
                fix.title.render_with(self.options)
            )?;
            fmt_fix(f, fix, &self.options)?;
        }

        for trace in &d.trace {
            writeln!(f, "{}{} {}", GUTTER, subtext("!"), trace)?;
        }

        Ok(())
    }
}

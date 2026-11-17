use std::collections::HashSet;
use std::fmt::Display;

use owo_colors::OwoColorize;

use super::palette::*;
use super::text::strip_ansi;
use super::{Diagnostic, Fix, Label, Severity};

/// Colours one line of source code for display in a diagnostic.
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
    /// The diagnostic for a terminal (`color`), or as plain text with names and
    /// code between backticks.
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

/// The gutter prefix: 4 chars for line number + space + pipe + space = "     │ "
const GUTTER: &str = "     ";

/// Render a group of labels from the same source file.
fn fmt_label_group(
    f: &mut std::fmt::Formatter<'_>,
    group: &[&Label],
    severity: &Severity,
    options: &RenderOptions,
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
                    margin(format!("{:>4}", line_num))
                }
            };

            writeln!(
                f,
                "{} {} {}",
                line_color,
                surface("│"),
                options.highlight(line_text)
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
                    label.message.render_with(*options)
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

/// A fix as a diff of the lines it changes. Each edit replaces a whole node (a name, a
/// path) or inserts whole lines, so the removed and added parts are exactly the edits,
/// with the rest of the line highlighted as usual.
fn fmt_fix(
    f: &mut std::fmt::Formatter<'_>,
    fix: &Fix,
    options: &RenderOptions,
) -> std::fmt::Result {
    let line_number =
        |n: usize, rgb: Rgb| format!("{}", format!("{n:>4}").truecolor(rgb.0, rgb.1, rgb.2));
    let paint = |text: &str, rgb: Rgb| {
        format!("{}", text.truecolor(rgb.0, rgb.1, rgb.2).bold().underline())
    };
    let marker = |text: &str, rgb: Rgb| format!("{}", text.truecolor(rgb.0, rgb.1, rgb.2).bold());

    // Edits on the same line are shown together, as the line comes out: `&(n + 1)` is one
    // change, not a `&(` and a `)`.
    let mut shown: Vec<usize> = Vec::new();
    for (i, (span, text)) in fix.edits.iter().enumerate() {
        if shown.contains(&i) {
            continue;
        }
        let content = span.source.content();
        let start = span.start_offset;
        let end = start + span.length;
        let line_start = content[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = content[end..].find('\n').map_or(content.len(), |i| end + i);
        let line = span.start.line;

        // Whole lines put in front of a line: `extern "raylib";`.
        if span.length == 0 && start == line_start && text.ends_with('\n') {
            for (i, added) in text.lines().enumerate() {
                writeln!(
                    f,
                    "{} {} {}",
                    line_number(line + i, GREEN),
                    marker("+", GREEN),
                    paint(added, GREEN)
                )?;
            }
            if let Some(next) = content[line_start..].lines().next() {
                let n = line + text.lines().count();
                writeln!(
                    f,
                    "{} {}   {}",
                    margin(format!("{n:>4}")),
                    surface("│"),
                    options.highlight(next)
                )?;
            }
            continue;
        }

        // An edit across lines, like removing a declaration or replacing a block: the
        // lines it touches, then what they become.
        if content[start..end].contains('\n') || text.contains('\n') {
            shown.push(i);
            // Ending just after a newline leaves the next line alone.
            let last = if end > start && content[..end].ends_with('\n') {
                end - 1
            } else {
                line_end
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
                    paint(&content[cut_from..cut_to], RED),
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
                    writeln!(
                        f,
                        "{} {} {}",
                        line_number(line + n, GREEN),
                        marker("+", GREEN),
                        paint(added, GREEN)
                    )?;
                }
            }
            continue;
        }

        // Every edit on this line, in order along it.
        let mut on_line: Vec<(usize, usize, &str)> = (fix.edits.iter().enumerate())
            .filter(|(j, (other, other_text))| {
                !shown.contains(j)
                    && other.source.identifier() == span.source.identifier()
                    && other.start.line == line
                    && !(other.length == 0
                        && other.start_offset == line_start
                        && other_text.ends_with('\n'))
            })
            .map(|(j, (other, other_text))| {
                (
                    j,
                    other.start_offset,
                    other.start_offset + other.length,
                    other_text.as_str(),
                )
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(j, at, until, inserted)| {
                shown.push(j);
                (at, until, inserted)
            })
            .collect();
        on_line.sort_by_key(|&(at, ..)| at);

        let (mut removed_line, mut added_line) = (String::new(), String::new());
        let mut at = line_start;
        for &(edit_start, edit_end, inserted) in &on_line {
            let kept = options.highlight(&content[at..edit_start]);
            removed_line.push_str(&kept);
            removed_line.push_str(&paint(&content[edit_start..edit_end], RED));
            added_line.push_str(&kept);
            added_line.push_str(&paint(inserted, GREEN));
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
        )?;
    }
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

        // Each fix: what it does, then the lines as it leaves them.
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

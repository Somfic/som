//! Between som's spans and diagnostics and the protocol's ranges and diagnostics.

use som_base::{Diagnostic as SomDiagnostic, Fix, Severity as SomSeverity, Span};
use tower_lsp::lsp_types::{
    self, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Position, Range, Url,
};

/// Som's lines and columns count from 1, the protocol's from 0.
pub fn span_to_range(span: &Span) -> Range {
    Range {
        start: Position::new(
            (span.start.line.saturating_sub(1)) as u32,
            (span.start.col.saturating_sub(1)) as u32,
        ),
        end: Position::new(
            (span.end.line.saturating_sub(1)) as u32,
            (span.end.col.saturating_sub(1)) as u32,
        ),
    }
}

pub fn location(span: &Span) -> Option<Location> {
    Some(Location {
        uri: Url::from_file_path(span.source.identifier()).ok()?,
        range: span_to_range(span),
    })
}

fn severity_to_lsp(severity: &SomSeverity) -> DiagnosticSeverity {
    match severity {
        SomSeverity::Error => DiagnosticSeverity::ERROR,
        SomSeverity::Warning => DiagnosticSeverity::WARNING,
        SomSeverity::Note | SomSeverity::Success(_) => DiagnosticSeverity::INFORMATION,
        SomSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

/// A diagnostic on its primary label, with its labels as related information, and each of
/// its hints as an information diagnostic of its own. With the file it's in.
pub fn som_diagnostic_to_lsp(diag: &SomDiagnostic) -> Option<(String, Vec<lsp_types::Diagnostic>)> {
    let severity = severity_to_lsp(&diag.severity);
    let primary = diag
        .labels
        .iter()
        .find(|l| l.is_primary)
        .or_else(|| diag.labels.first())?;

    let file = primary.span.source.identifier().to_string();
    let range = span_to_range(&primary.span);
    let message = diag.message.plain();

    let related: Vec<DiagnosticRelatedInformation> = diag
        .labels
        .iter()
        .filter(|l| !l.message.is_empty())
        .filter_map(|l| {
            Some(DiagnosticRelatedInformation {
                location: location(&l.span)?,
                message: l.message.plain(),
            })
        })
        .collect();

    let mut diagnostics = vec![lsp_types::Diagnostic {
        range,
        severity: Some(severity),
        message,
        source: Some("som".to_string()),
        related_information: if related.is_empty() {
            None
        } else {
            Some(related)
        },
        data: (!diag.fixes.is_empty()).then(|| fixes_to_json(&diag.fixes)),
        ..Default::default()
    }];

    for hint in &diag.hints {
        diagnostics.push(lsp_types::Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::INFORMATION),
            message: hint.plain(),
            source: Some("som".to_string()),
            ..Default::default()
        });
    }

    Some((file, diagnostics))
}

/// A diagnostic's fixes, kept in its `data` so a code action request can apply them:
/// `[{"title": …, "edits": [{"uri": …, "range": …, "newText": …}]}]`.
fn fixes_to_json(fixes: &[Fix]) -> serde_json::Value {
    serde_json::Value::Array(
        fixes
            .iter()
            .map(|fix| {
                let edits: Vec<serde_json::Value> = fix
                    .edits
                    .iter()
                    .filter_map(|(span, text)| {
                        Some(serde_json::json!({
                            "uri": Url::from_file_path(span.source.identifier()).ok()?,
                            "range": span_to_range(span),
                            "newText": text,
                        }))
                    })
                    .collect();
                serde_json::json!({ "title": fix.title.plain(), "edits": edits })
            })
            .collect(),
    )
}

/// The code actions for a diagnostic's fixes, from its `data`.
pub fn fixes_from_json(diagnostic: &lsp_types::Diagnostic) -> Vec<lsp_types::CodeActionOrCommand> {
    let Some(serde_json::Value::Array(fixes)) = &diagnostic.data else {
        return Vec::new();
    };
    fixes
        .iter()
        .enumerate()
        .filter_map(|(i, fix)| {
            let mut changes: std::collections::HashMap<Url, Vec<lsp_types::TextEdit>> =
                Default::default();
            for edit in fix["edits"].as_array()? {
                let uri: Url = serde_json::from_value(edit["uri"].clone()).ok()?;
                changes.entry(uri).or_default().push(lsp_types::TextEdit {
                    range: serde_json::from_value(edit["range"].clone()).ok()?,
                    new_text: edit["newText"].as_str()?.to_string(),
                });
            }
            Some(lsp_types::CodeActionOrCommand::CodeAction(
                lsp_types::CodeAction {
                    title: fix["title"].as_str()?.to_string(),
                    kind: Some(lsp_types::CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    edit: Some(lsp_types::WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    // The first fix is the likeliest, the one to apply with a keypress.
                    is_preferred: Some(i == 0),
                    ..Default::default()
                },
            ))
        })
        .collect()
}

/// Where byte `offset` of `text` is, counting columns in characters as `position_to_offset`
/// does.
pub fn offset_to_position(text: &str, offset: usize) -> Position {
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let column = before[before.rfind('\n').map_or(0, |i| i + 1)..]
        .chars()
        .count();
    Position::new(line as u32, column as u32)
}

/// The byte offset of `position` in `text`. A column past the end of its line is the line's
/// end.
pub fn position_to_offset(text: &str, position: &Position) -> Option<usize> {
    let target_line = position.line as usize;
    let target_col = position.character as usize;

    let mut current_line = 0;
    let mut current_col = 0;

    for (offset, ch) in text.char_indices() {
        if current_line == target_line && current_col == target_col {
            return Some(offset);
        }
        if ch == '\n' {
            if current_line == target_line {
                return Some(offset);
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }

    if current_line == target_line && current_col == target_col {
        return Some(text.len());
    }

    None
}

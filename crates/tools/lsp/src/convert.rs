use som_base::{Diagnostic as SomDiagnostic, Severity as SomSeverity, Span};
use tower_lsp::lsp_types::{
    self, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Position, Range, Url,
};

/// Convert a Som span (1-indexed) to an LSP range (0-indexed)
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

/// Convert a Som severity to an LSP severity
fn severity_to_lsp(severity: &SomSeverity) -> DiagnosticSeverity {
    match severity {
        SomSeverity::Error => DiagnosticSeverity::ERROR,
        SomSeverity::Warning => DiagnosticSeverity::WARNING,
        SomSeverity::Note | SomSeverity::Success(_) => DiagnosticSeverity::INFORMATION,
    }
}

/// Convert a Som Diagnostic to LSP diagnostics on the primary label.
/// Returns the main diagnostic plus separate HINT diagnostics for the label and hints.
pub fn som_diagnostic_to_lsp(diag: &SomDiagnostic) -> Option<(String, Vec<lsp_types::Diagnostic>)> {
    let severity = severity_to_lsp(&diag.severity);

    // Find the primary label, or fall back to the first label
    let primary = diag
        .labels
        .iter()
        .find(|l| l.is_primary)
        .or_else(|| diag.labels.first())?;

    let file = primary.span.source.identifier().to_string();
    let range = span_to_range(&primary.span);
    let message = diag.message.plain();

    // Attach all labels as related information
    let related: Vec<DiagnosticRelatedInformation> = diag
        .labels
        .iter()
        .filter(|l| !l.message.is_empty())
        .filter_map(|l| {
            let uri = Url::from_file_path(l.span.source.identifier()).ok()?;
            Some(DiagnosticRelatedInformation {
                location: Location {
                    uri,
                    range: span_to_range(&l.span),
                },
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
        ..Default::default()
    }];

    // Emit each hint as a separate information diagnostic
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

/// Convert an LSP position to a byte offset in source text
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
                // Column is past end of line
                return Some(offset);
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }

    // Position at end of file
    if current_line == target_line && current_col == target_col {
        return Some(text.len());
    }

    None
}

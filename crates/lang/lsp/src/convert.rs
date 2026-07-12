//! Conversions between som's byte-offset world and LSP's line/UTF-16-column
//! world. Everything here is pure and document-local: given the text of a
//! document, we can map any span into it.

use som_common::{Diagnostic as SomDiagnostic, Label, Severity, Span};
use tower_lsp::lsp_types::{
    self, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Position, Range, Url,
};

/// Byte offset -> LSP position index for a single document.
///
/// LSP positions are 0-indexed `(line, character)` where `character` counts
/// UTF-16 code units, so we can't reuse `Source`'s byte columns directly.
pub struct LineIndex<'a> {
    text: &'a str,
    /// Byte offset of the start of each line. Always begins with `0`.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { text, line_starts }
    }

    /// LSP position for a byte offset, clamped into the document.
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        // Column is the number of UTF-16 code units between the line start and
        // the offset. Spans come from the lexer so `offset` lands on a char
        // boundary; `encode_utf16().count()` handles surrogate pairs.
        let col = self.text[line_start..offset].encode_utf16().count();
        Position::new(line as u32, col as u32)
    }

    pub fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start as usize),
            end: self.position(span.end as usize),
        }
    }
}

fn severity_to_lsp(severity: Severity) -> DiagnosticSeverity {
    match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Note => DiagnosticSeverity::INFORMATION,
        Severity::Help => DiagnosticSeverity::HINT,
    }
}

/// Build the related-information entry for a secondary label, if it carries a
/// message.
fn related(uri: &Url, index: &LineIndex, label: &Label) -> Option<DiagnosticRelatedInformation> {
    if label.message.is_empty() {
        return None;
    }
    Some(DiagnosticRelatedInformation {
        location: Location {
            uri: uri.clone(),
            range: index.range(label.span),
        },
        message: label.message.clone(),
    })
}

/// Convert one som diagnostic into an LSP diagnostic anchored at its primary
/// span. Secondary labels become related information; the primary label, notes,
/// and suggestions are folded into the message so they show up on hover.
pub fn to_lsp(uri: &Url, index: &LineIndex, diag: &SomDiagnostic) -> lsp_types::Diagnostic {
    let mut message = diag.message.plain();
    if !diag.primary.message.is_empty() {
        message.push_str("\n\n");
        message.push_str(&diag.primary.message);
    }
    for note in &diag.notes {
        message.push_str("\n\nnote: ");
        message.push_str(note);
    }
    for suggestion in &diag.suggestions {
        message.push_str("\n\nhelp: ");
        message.push_str(&suggestion.message);
    }

    let related: Vec<_> = diag
        .secondary
        .iter()
        .filter_map(|label| related(uri, index, label))
        .collect();

    lsp_types::Diagnostic {
        range: index.range(diag.primary.span),
        severity: Some(severity_to_lsp(diag.severity)),
        code: diag
            .code
            .map(|c| lsp_types::NumberOrString::String(c.to_string())),
        source: Some("som".to_string()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        ..Default::default()
    }
}

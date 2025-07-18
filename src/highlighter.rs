use owo_colors::{Style, Styled};

use crate::prelude::*;

pub struct SomHighlighter {}
struct SomHighlighterState {}

impl miette::highlighters::Highlighter for SomHighlighter {
    fn start_highlighter_state<'h>(
        &'h self,
        _source: &dyn miette::SpanContents<'_>,
    ) -> Box<dyn miette::highlighters::HighlighterState + 'h> {
        Box::new(SomHighlighterState {})
    }
}

impl miette::highlighters::HighlighterState for SomHighlighterState {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let tokenizer = Lexer::new(line);
        let tokens: Vec<_> = tokenizer.into_iter().collect();

        // If there are any errors in tokenizing, just return the plain text with a color that's visible on dark backgrounds
        if tokens.iter().any(|t| t.is_err()) {
            return vec![Style::new()
                .remove_all_effects()
                .fg_rgb::<210, 219, 245>()
                .style(line)];
        }

        // Process tokens into styled segments
        let mut sections = Vec::new();
        let mut current_pos = 0;

        for token_result in tokens {
            if let Ok(token) = token_result {
                let start = token.span.0.offset();
                let end = start + token.span.0.len();

                // If there's a gap between the last token and this one, add it with default style
                if start > current_pos {
                    let gap = &line[current_pos..start];
                    if !gap.is_empty() {
                        sections.push(Style::new().fg_rgb::<210, 219, 245>().style(gap));
                    }
                }

                // Style the current token using optimized Catppuccin Mocha palette for dark backgrounds
                let style = match token.kind {
                    // Keywords - Mauve (slightly brightened for better contrast)
                    TokenKind::If
                    | TokenKind::Else
                    | TokenKind::Let
                    | TokenKind::Type
                    | TokenKind::Struct
                    | TokenKind::Enum
                    | TokenKind::Function
                    | TokenKind::Extern
                    | TokenKind::Trait
                    | TokenKind::While
                    | TokenKind::For
                    | TokenKind::Use
                    | TokenKind::Mod
                    | TokenKind::Return => Style::new().fg_rgb::<205, 169, 250>().bold(),

                    // Identifiers - Lavender (slightly brightened)
                    TokenKind::Identifier => Style::new().fg_rgb::<188, 197, 255>(),

                    // Strings - Green (optimized for dark background)
                    TokenKind::String | TokenKind::Character => {
                        Style::new().fg_rgb::<171, 233, 164>().italic()
                    }

                    // Numbers - Peach (optimized for visibility)
                    TokenKind::I32 | TokenKind::I64 | TokenKind::Decimal => {
                        Style::new().fg_rgb::<250, 183, 142>()
                    }

                    // Booleans - Sky (slightly saturated for better visibility)
                    TokenKind::Boolean => Style::new().fg_rgb::<140, 224, 240>().bold(),

                    // Types - Teal (brightened slightly for better contrast)
                    TokenKind::I32Type
                    | TokenKind::I64Type
                    | TokenKind::DecimalType
                    | TokenKind::BooleanType
                    | TokenKind::UnitType
                    | TokenKind::StringType
                    | TokenKind::CharacterType => Style::new().fg_rgb::<156, 235, 220>().italic(),

                    // Operators - Subtext0 (brightened for better visibility)
                    TokenKind::Equal
                    | TokenKind::LessThan
                    | TokenKind::GreaterThan
                    | TokenKind::LessThanOrEqual
                    | TokenKind::GreaterThanOrEqual
                    | TokenKind::Equality
                    | TokenKind::Inequality
                    | TokenKind::Percent
                    | TokenKind::Not
                    | TokenKind::And
                    | TokenKind::Pipe
                    | TokenKind::Caret
                    | TokenKind::Or => Style::new().fg_rgb::<200, 208, 237>(),

                    // Punctuation - Overlay1 (brightened slightly for better readability)
                    TokenKind::Semicolon | TokenKind::Comma => {
                        Style::new().fg_rgb::<140, 145, 170>()
                    }

                    // Everything else - Text (optimized for dark background)
                    _ => Style::new().fg_rgb::<210, 219, 245>(),
                };

                // Get the actual text from the line rather than token.original
                // to ensure we're highlighting exactly what was in the source
                let token_text = &line[start..end];
                sections.push(style.style(token_text));

                // Update current position
                current_pos = end;
            }
        }

        // If there's any remaining text after the last token, add it with default style
        if current_pos < line.len() {
            let remaining = &line[current_pos..];
            if !remaining.is_empty() {
                sections.push(Style::new().fg_rgb::<210, 219, 245>().style(remaining));
            }
        }

        sections
    }
}

use owo_colors::{Style, Styled};

use crate::tokenizer::{TokenKind, Tokenizer};

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
        let mut sections: Vec<Styled<&'s str>> = vec![];

        for word in line.split(' ') {
            for token in Tokenizer::<'s>::new(word) {
                let style: Style = match &token {
                    Ok(token) => match &token.kind {
                        // Comment / quote -> 92, 99, 112 + italic
                        TokenKind::If
                        | TokenKind::Else
                        | TokenKind::Let
                        | TokenKind::Type
                        | TokenKind::Struct
                        | TokenKind::Enum
                        | TokenKind::Function
                        | TokenKind::Intrinsic
                        | TokenKind::Trait
                        | TokenKind::While
                        | TokenKind::For
                        | TokenKind::Use
                        | TokenKind::Mod
                        | TokenKind::Return => Style::new().fg_rgb::<197, 120, 221>().bold(),
                        TokenKind::Identifier => Style::new().fg_rgb::<224, 108, 117>(),
                        TokenKind::String | TokenKind::Character => {
                            Style::new().fg_rgb::<152, 195, 121>().italic()
                        }
                        TokenKind::Integer | TokenKind::Decimal => {
                            Style::new().fg_rgb::<209, 154, 102>()
                        }
                        TokenKind::Boolean => Style::new().fg_rgb::<86, 156, 214>(),
                        TokenKind::IntegerType
                        | TokenKind::DecimalType
                        | TokenKind::BooleanType
                        | TokenKind::UnitType
                        | TokenKind::StringType
                        | TokenKind::CharacterType => {
                            Style::new().fg_rgb::<86, 156, 214>().italic()
                        }
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
                        | TokenKind::Or
                        | TokenKind::Semicolon
                        | TokenKind::Comma => Style::new().fg_rgb::<200, 200, 200>(),
                        _ => Style::new().fg_rgb::<171, 178, 191>(),
                    },
                    Err(_) => return vec![Style::new().remove_all_effects().white().style(line)],
                };

                let token = token.unwrap();
                sections.push(style.style(token.original));
            }
            sections.push(Style::new().remove_all_effects().style(" "));
        }

        sections
    }
}

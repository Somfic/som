use std::vec;

use lexer::{Lexer, TokenKind};
use miette::{highlighters::Highlighter, LabeledSpan};
use owo_colors::{colors, styles, OwoColorize, Style, Styled};
use parser::Parser;

pub mod lexer;
pub mod parser;

fn main() {
    let input = "let value = a == { let b = 1 if true else 2 };";

    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(3)
                .with_syntax_highlighting(SomHighlighter {})
                .build(),
        )
    }))
    .unwrap();

    let mut parser = Parser::new(input);
    let symbol = match parser.parse() {
        Ok(symbol) => symbol,
        Err(err) => {
            println!("{:?}", err.with_source_code(input.to_string()));
            return;
        }
    };

    println!("{:?}", symbol);
}

struct SomHighlighter {}
struct SomHighlighterState {}

impl miette::highlighters::Highlighter for SomHighlighter {
    fn start_highlighter_state<'h>(
        &'h self,
        source: &dyn miette::SpanContents<'_>,
    ) -> Box<dyn miette::highlighters::HighlighterState + 'h> {
        Box::new(SomHighlighterState {})
    }
}

impl miette::highlighters::HighlighterState for SomHighlighterState {
    fn highlight_line<'s>(&mut self, line: &'s str) -> Vec<Styled<&'s str>> {
        let mut sections: Vec<Styled<&'s str>> = vec![];

        for word in line.split(' ') {
            for token in Lexer::<'s>::new(word) {
                let style: Style = match &token {
                    Ok(token) => match &token.kind {
                        // Comment / quote -> 92, 99, 112 + italic
                        TokenKind::If | TokenKind::Else | TokenKind::Let => {
                            Style::new().fg_rgb::<197, 120, 221>()
                        }
                        TokenKind::Identifier => Style::new().fg_rgb::<224, 108, 117>(),
                        TokenKind::String => Style::new().fg_rgb::<152, 195, 121>().italic(),
                        TokenKind::Integer | TokenKind::Decimal => {
                            Style::new().fg_rgb::<209, 154, 102>()
                        }
                        TokenKind::Boolean => Style::new().fg_rgb::<86, 156, 214>(),
                        TokenKind::CurlyOpen
                        | TokenKind::CurlyClose
                        | TokenKind::ParenOpen
                        | TokenKind::ParenClose
                        | TokenKind::SquareOpen
                        | TokenKind::SquareClose
                        | TokenKind::Equal
                        | TokenKind::LessThan
                        | TokenKind::GreaterThan
                        | TokenKind::LessThanOrEqual
                        | TokenKind::GreaterThanOrEqual
                        | TokenKind::Equality
                        | TokenKind::Inequality
                        | TokenKind::Plus
                        | TokenKind::Minus
                        | TokenKind::Star
                        | TokenKind::Slash
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

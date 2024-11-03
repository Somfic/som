use lexer::{Lexer, TokenKind};
use owo_colors::{Style, Styled};
use parser::{
    ast::{Expression, ExpressionValue, Statement},
    Parser,
};
use passer::{typing::Typing, Passer};
use std::vec;

pub mod lexer;
pub mod parser;
pub mod passer;

const INPUT: &str = "
fn main() {
    let string = \"Hello, world!\";
    return 12;

    {
        let string = \"Hello, world!\";
        return 12;
    };

    let abc = 12;
}
";

fn main() {
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

    let mut parser = Parser::new(INPUT);
    let symbol = match parser.parse() {
        Ok(symbol) => symbol,
        Err(err) => {
            println!("{:?}", err.with_source_code(INPUT));
            return;
        }
    };

    let typing_pass = passer::typing::TypingPasser::pass(&symbol).unwrap();
    let typing_pass = typing_pass.combine(passer::unused::UnusedPass::pass(&symbol).unwrap());

    for note in typing_pass.non_critical {
        println!("{:?}", note.with_source_code(INPUT));
    }

    for note in typing_pass.critical {
        println!("{:?}", note.with_source_code(INPUT));
    }
}

struct SomHighlighter {}
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
            for token in Lexer::<'s>::new(word) {
                let style: Style = match &token {
                    Ok(token) => match &token.kind {
                        // Comment / quote -> 92, 99, 112 + italic
                        TokenKind::If
                        | TokenKind::Else
                        | TokenKind::Let
                        | TokenKind::Struct
                        | TokenKind::Enum
                        | TokenKind::Function
                        | TokenKind::Trait
                        | TokenKind::Return => Style::new().fg_rgb::<197, 120, 221>(),
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

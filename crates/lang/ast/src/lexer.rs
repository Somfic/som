use logos::Logos;
use som_common::*;

use crate::{Token, TokenKind};

pub fn lex(source: Id<Source>, content: &str) -> Vec<Token> {
    layout(tokenize(source, content), content)
}

fn tokenize(source: Id<Source>, content: &str) -> Vec<Token> {
    debug!(source = ?source, "Lexing");

    let mut lexer = TokenKind::lexer(content);
    let mut tokens = Vec::new();

    while let Some(result) = lexer.next() {
        let kind = result.unwrap_or(TokenKind::Error);
        let span_range = lexer.span();
        let text = lexer.slice();

        if kind == TokenKind::Whitespace {
            continue;
        }

        tokens.push(Token {
            kind,
            text: text.into(),
            span: Span::from_range(source, span_range),
        });
    }

    // EOF
    let eof_pos = content.len();
    tokens.push(Token {
        kind: TokenKind::Eof,
        text: "".into(),
        span: Span::from_range(source, eof_pos..eof_pos),
    });

    debug!(source = ?source, token_count = tokens.len(), "Lexing complete");

    if std::env::var("SOM_DUMP_TOKENS").is_ok() {
        info!("tokens dump:\n{tokens:#?}");
    }

    tokens
}

fn layout(tokens: Vec<Token>, content: &str) -> Vec<Token> {
    let source = tokens.first().map(|t| t.span.source).unwrap_or(Id::new(0));
    let eof_pos = content.len();

    let synth = |kind: TokenKind, pos: usize| Token {
        kind,
        text: "".into(),
        span: Span::from_range(source, pos..pos),
    };
    let col_of = |offset: u32| -> usize {
        let o = offset as usize;
        let line_start = content[..o].rfind('\n').map(|i| i + 1).unwrap_or(0);
        o - line_start
    };
    let line_of = |offset: u32| -> usize {
        content[..offset as usize]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
    };

    // group real tokens into logical lines by the line their start falls on
    let mut lines: Vec<Vec<Token>> = Vec::new();
    let mut current_line: Option<usize> = None;
    for tok in tokens {
        if tok.kind == TokenKind::Eof {
            continue;
        }
        let line = line_of(tok.span.start);
        if current_line == Some(line) {
            lines.last_mut().unwrap().push(tok);
        } else {
            current_line = Some(line);
            lines.push(vec![tok]);
        }
    }

    let mut out = Vec::new();
    let mut stack = vec![0usize];
    let mut first = true;

    for line in lines {
        let head = &line[0];
        // comment-only lines never influence indentation
        if head.kind == TokenKind::Comment {
            out.extend(line);
            continue;
        }
        let pos = head.span.start as usize;
        let col = col_of(head.span.start);
        let top = *stack.last().unwrap();

        if col > top {
            stack.push(col);
            out.push(synth(TokenKind::Indent, pos));
        } else if col < top {
            // unwind closed blocks until we land on the enclosing indent level
            while *stack.last().unwrap() > col {
                stack.pop();
                out.push(synth(TokenKind::Dedent, pos));
            }
            if *stack.last().unwrap() == col {
                out.push(synth(TokenKind::Newline, pos));
            } else {
                out.push(synth(TokenKind::Error, pos));
            }
        } else if !first {
            out.push(synth(TokenKind::Newline, pos));
        }

        first = false;
        out.extend(line);
    }

    // close any still-open blocks at end of input
    while stack.len() > 1 {
        stack.pop();
        out.push(synth(TokenKind::Dedent, eof_pos));
    }
    out.push(synth(TokenKind::Eof, eof_pos));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TokenKind::*;

    fn kinds(content: &str) -> Vec<TokenKind> {
        lex(Id::new(0), content)
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn counter_program_layout() {
        let src = "\
let count = 0

main
  button @click: count += 1
    \"+1\"
  \"count: {count}\"
  \"doubled: {count * 2}\"
";
        assert_eq!(
            kinds(src),
            vec![
                Let, Ident, Equals, Int, // let count = 0
                Newline, Ident, // main
                Indent, Ident, At, Ident, Colon, Ident, PlusEquals,
                Int, // button @click: count += 1
                Indent, Text, // "+1"
                Dedent, Newline, Text, // "count: {count}"
                Newline, Text, // "doubled: {count * 2}"
                Dedent, Eof,
            ]
        );
    }

    #[test]
    fn at_and_plus_equals() {
        assert_eq!(kinds("@ += "), vec![At, PlusEquals, Eof]);
    }

    #[test]
    fn flat_program_newlines() {
        assert_eq!(
            kinds("let x = 1\nlet y = 2\n"),
            vec![
                Let, Ident, Equals, Int, Newline, Let, Ident, Equals, Int, Eof
            ]
        );
    }
}

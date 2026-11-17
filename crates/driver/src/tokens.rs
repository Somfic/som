use som_ast::Ast;
use som_lexer::TokenKind;
use std::fmt::Write;
use std::path::Path;

/// Every token of every file in the program, for `--emit tokens`. Files are named
/// by module and file name, so the dump doesn't depend on where the program lives.
pub fn dump_tokens(ast: &Ast) -> String {
    let mut out = String::new();
    for module in &ast.modules {
        for source in &module.files {
            let file = Path::new(source.identifier())
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let name = if module.name.is_empty() {
                file.to_string()
            } else {
                format!("{}/{file}", module.name.replace("::", "/"))
            };
            let _ = writeln!(out, "file {name}");
            for token in som_lexer::lex(source.clone()) {
                if matches!(token.kind, TokenKind::Whitespace | TokenKind::Eof) {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "  {}:{} {:?} {:?}",
                    token.span.start.line, token.span.start.col, token.kind, &*token.text
                );
            }
        }
    }
    out
}

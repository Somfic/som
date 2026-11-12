use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use som_ast::{Expr, Func, Id, Item, Span, Stmt, Struct};
use som_base::Source;
use som_driver::Analysis;
use som_lexer::{TokenKind, lex};
use som_resolve::{DefId, DefKind};
use som_typeck::Ty;

use crate::convert;
use crate::index::{AstIndex, NodeRef, items_in_file};

/// The analysis of one project (a directory of `.som` files).
struct Project {
    analysis: Analysis,
    /// Files diagnostics were published for, so they can be cleared next time.
    files: HashSet<String>,
}

pub struct SomLanguageServer {
    client: Client,
    projects: RwLock<HashMap<PathBuf, Project>>,
}

/// Where a request points: the file, its text, and the byte offset of the position.
struct Cursor {
    file: String,
    text: String,
    offset: usize,
}

impl SomLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            projects: RwLock::new(HashMap::new()),
        }
    }

    fn file_of(uri: &Url) -> Option<String> {
        Some(uri.to_file_path().ok()?.to_string_lossy().into_owned())
    }

    fn cursor(uri: &Url, position: &lsp_types::Position) -> Option<Cursor> {
        let file = Self::file_of(uri)?;
        let text = std::fs::read_to_string(&file).ok()?;
        let offset = convert::position_to_offset(&text, position)?;
        Some(Cursor { file, text, offset })
    }

    /// The project a file belongs to: the one rooted at its directory, or any project that loaded it.
    fn project_for<'a>(
        projects: &'a HashMap<PathBuf, Project>,
        file: &str,
    ) -> Option<&'a Analysis> {
        let dir = Path::new(file).parent()?;
        projects
            .get(dir)
            .or_else(|| {
                projects
                    .values()
                    .find(|p| p.analysis.ast.modules.iter().any(|m| m.path == dir))
            })
            .map(|p| &p.analysis)
    }

    /// Analyzes the project containing `uri` and publishes its diagnostics.
    async fn analyze(&self, uri: &Url) {
        let Some(root) = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
        else {
            return;
        };
        let opened = Self::file_of(uri).unwrap_or_default();

        let analysis = som_driver::analyze(&root, &Default::default());

        // Every file the program was loaded from, so fixed files get their diagnostics cleared.
        let ast = &analysis.ast;
        let mut files: HashSet<String> = HashSet::from([opened.clone()]);
        let spans = ast
            .funcs
            .iter()
            .map(|(id, _)| ast.funcs.span(id))
            .chain(ast.structs.iter().map(|(id, _)| ast.structs.span(id)));
        files.extend(
            spans
                .map(|s| s.source.identifier().to_string())
                .filter(|f| !f.starts_with('<')),
        );

        let mut by_file: HashMap<String, Vec<lsp_types::Diagnostic>> =
            files.iter().map(|f| (f.clone(), Vec::new())).collect();
        for error in &analysis.errors {
            let diagnostic = error.to_diagnostic();
            match convert::som_diagnostic_to_lsp(&diagnostic) {
                Some((file, diagnostics)) => by_file.entry(file).or_default().extend(diagnostics),
                // Errors without a location, like an unreadable file, go on the file that was opened.
                None => by_file
                    .entry(opened.clone())
                    .or_default()
                    .push(lsp_types::Diagnostic {
                        message: diagnostic.message.plain(),
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("som".into()),
                        ..Default::default()
                    }),
            }
        }

        let mut projects = self.projects.write().await;
        if let Some(previous) = projects.get(&root) {
            for file in previous.files.difference(&files) {
                by_file.entry(file.clone()).or_default();
            }
        }
        for (file, diagnostics) in by_file {
            if let Ok(uri) = Url::from_file_path(&file) {
                self.client
                    .publish_diagnostics(uri, diagnostics, None)
                    .await;
            }
        }
        projects.insert(root, Project { analysis, files });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SomLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    ..Default::default()
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: TOKEN_TYPES.to_vec(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "som-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.analyze(&params.text_document.uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.analyze(&params.text_document.uri).await;
    }

    async fn did_change(&self, _: DidChangeTextDocumentParams) {
        // Analysis reads files from disk, so it runs on save rather than on every keystroke.
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = Self::cursor(&position.text_document.uri, &position.position) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };
        let Some(node) = AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset) else {
            return Ok(None);
        };

        let text = match node {
            NodeRef::Expr(id) => match &a.ast.exprs[id] {
                Expr::Var(_) | Expr::Call { .. } | Expr::Construct { .. } => {
                    a.names.expr(id).map(|def| describe(a, def))
                }
                Expr::MethodCall { .. } => a.typed.method(id).map(|def| describe(a, def)),
                Expr::Field { field, .. } => {
                    Some(format!(".{}: {}", field.name, ty(a, a.typed.expr(id))))
                }
                _ => Some(ty(a, a.typed.expr(id))),
            },
            NodeRef::Type(id) => a.names.ty(id).map(|def| describe(a, def)),
            NodeRef::Func(id) => Some(signature(a, id)),
            NodeRef::Struct(id) => Some(struct_text(a, id)),
            NodeRef::Stmt(id) => match &a.ast.stmts[id] {
                Stmt::Let { .. } => a.names.local(id).map(|def| describe(a, def)),
                _ => None,
            },
        };

        Ok(text.filter(|t| !t.contains("{error}")).map(|text| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```som\n{text}\n```"),
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = Self::cursor(&position.text_document.uri, &position.position) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };
        let Some(node) = AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset) else {
            return Ok(None);
        };

        let target: Option<Span> = match node {
            NodeRef::Expr(id) => match &a.ast.exprs[id] {
                Expr::Var(_) | Expr::Call { .. } | Expr::Construct { .. } => {
                    a.names.expr(id).map(|d| a.names.def(d).span.clone())
                }
                Expr::MethodCall { .. } => a.typed.method(id).map(|d| a.names.def(d).span.clone()),
                Expr::Field { object, .. } => field_definition(a, *object, a.typed.field(id)),
                _ => None,
            },
            NodeRef::Type(id) => a.names.ty(id).map(|d| a.names.def(d).span.clone()),
            _ => None,
        };

        Ok(target
            .and_then(|span| location(&span))
            .map(GotoDefinitionResponse::Scalar))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(file) = Self::file_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &file) else {
            return Ok(None);
        };
        let ast = &a.ast;

        let func_symbol = |id: Id<Func>, kind: SymbolKind| {
            symbol(
                ast.funcs[id].name.name.to_string(),
                Some(signature(a, id)),
                kind,
                ast.funcs.span(id),
                &ast.funcs[id].name.span,
                None,
            )
        };

        let mut symbols = Vec::new();
        for item in items_in_file(ast, &file) {
            match item {
                Item::Func(id) => symbols.push(func_symbol(*id, SymbolKind::FUNCTION)),
                Item::Extern(block) => symbols.extend(
                    block
                        .funcs
                        .iter()
                        .map(|&id| func_symbol(id, SymbolKind::FUNCTION)),
                ),
                Item::Struct(id) => {
                    let s = &ast.structs[*id];
                    let fields = s
                        .fields
                        .iter()
                        .zip(a.typed.fields(*id))
                        .map(|(f, &t)| {
                            let span = f.name.span.merge(ast.types.span(f.ty));
                            symbol(
                                f.name.name.to_string(),
                                Some(ty(a, t)),
                                SymbolKind::FIELD,
                                &span,
                                &f.name.span,
                                None,
                            )
                        })
                        .collect();
                    symbols.push(symbol(
                        s.name.name.to_string(),
                        None,
                        SymbolKind::STRUCT,
                        ast.structs.span(*id),
                        &s.name.span,
                        Some(fields),
                    ));
                }
                Item::Impl(block) => {
                    let methods: Vec<DocumentSymbol> = block
                        .methods
                        .iter()
                        .map(|&m| func_symbol(m, SymbolKind::METHOD))
                        .collect();
                    let span = block
                        .methods
                        .last()
                        .map_or(block.target.span.clone(), |&m| {
                            block.target.span.merge(ast.funcs.span(m))
                        });
                    symbols.push(symbol(
                        format!("impl {}", block.target.name),
                        None,
                        SymbolKind::OBJECT,
                        &span,
                        &block.target.span,
                        Some(methods),
                    ));
                }
                Item::Use(_) => {}
            }
        }
        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        let Some(file) = Self::file_of(uri) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &file) else {
            return Ok(None);
        };

        // The entry point: `main` in the root module.
        let main = items_in_file(&a.ast, &file)
            .into_iter()
            .find_map(|item| match item {
                Item::Func(id)
                    if &*a.ast.funcs[*id].name.name == "main"
                        && a.names.def(a.names.func(*id)).module == 0 =>
                {
                    Some(*id)
                }
                _ => None,
            });
        let Some(main) = main else {
            return Ok(None);
        };

        let start = convert::span_to_range(a.ast.funcs.span(main)).start;
        Ok(Some(vec![CodeLens {
            range: Range { start, end: start },
            command: Some(Command {
                title: "$(play) Run".to_string(),
                command: "som.run".to_string(),
                arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
            }),
            data: None,
        }]))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Ok(path) = params.text_document.uri.to_file_path() else {
            return Ok(None);
        };
        let Ok(source) = Source::from_file(&path) else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(Arc::new(source)),
        })))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let position = &params.text_document_position;
        let Some(cursor) = Self::cursor(&position.text_document.uri, &position.position) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };

        let mut items = Vec::new();
        let after_dot =
            cursor.offset > 0 && cursor.text.as_bytes().get(cursor.offset - 1) == Some(&b'.');

        if after_dot {
            // Fields and methods of the struct before the dot: the outermost expression
            // ending there, looking past spaces and closing parentheses.
            let dot = cursor.offset - 1;
            let end = cursor.text[..dot]
                .trim_end_matches(|c: char| c.is_whitespace() || c == ')')
                .len();
            let receiver = a
                .ast
                .exprs
                .iter()
                .map(|(id, _)| (id, a.ast.exprs.span(id)))
                .filter(|(_, span)| {
                    span.source.identifier() == cursor.file
                        && span.start_offset + span.length == end
                })
                .max_by_key(|(_, span)| span.length)
                .map(|(id, _)| id);
            if let Some(expr) = receiver
                && let Some(owner) = struct_of(a, a.typed.expr(expr))
                && let DefKind::Struct(s) = a.names.def(owner).kind
            {
                for (field, &t) in a.ast.structs[s].fields.iter().zip(a.typed.fields(s)) {
                    items.push(CompletionItem {
                        label: field.name.name.to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(ty(a, t)),
                        ..Default::default()
                    });
                }
                for (name, method) in a.names.methods_of(owner) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(describe(a, method)),
                        ..Default::default()
                    });
                }
            }
        } else {
            for module in &a.ast.modules {
                for item in &module.items {
                    let funcs: &[Id<Func>] = match item {
                        Item::Func(id) => std::slice::from_ref(id),
                        Item::Extern(block) => &block.funcs,
                        Item::Struct(id) => {
                            items.push(CompletionItem {
                                label: a.ast.structs[*id].name.name.to_string(),
                                kind: Some(CompletionItemKind::STRUCT),
                                ..Default::default()
                            });
                            &[]
                        }
                        _ => &[],
                    };
                    for &f in funcs {
                        items.push(CompletionItem {
                            label: a.ast.funcs[f].name.name.to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            detail: Some(signature(a, f)),
                            ..Default::default()
                        });
                    }
                }
            }
            for keyword in [
                "fn", "let", "mut", "if", "else", "struct", "impl", "extern", "loop", "while",
                "use", "true", "false",
            ] {
                items.push(CompletionItem {
                    label: keyword.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    ..Default::default()
                });
            }
        }
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let Some(file) = Self::file_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &file) else {
            return Ok(None);
        };

        // The inferred type after every unannotated `let`.
        let mut hints = Vec::new();
        for (id, stmt) in a.ast.stmts.iter() {
            let Stmt::Let { name, ty: None, .. } = stmt else {
                continue;
            };
            if name.span.source.identifier() != file {
                continue;
            }
            let Some(t) = a.names.local(id).and_then(|def| a.typed.try_local(def)) else {
                continue;
            };
            if matches!(a.typed.ty(t), Ty::Unit | Ty::Error) {
                continue;
            }
            hints.push(InlayHint {
                position: convert::span_to_range(&name.span).end,
                label: InlayHintLabel::String(format!(": {}", ty(a, t))),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: None,
                padding_right: None,
                data: None,
            });
        }
        Ok(Some(hints))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = Self::cursor(&position.text_document.uri, &position.position) else {
            return Ok(None);
        };
        let projects = self.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };

        // The innermost call around the cursor, and the function it calls.
        let call = a
            .ast
            .exprs
            .iter()
            .filter(|(id, e)| {
                let span = a.ast.exprs.span(*id);
                matches!(e, Expr::Call { .. } | Expr::MethodCall { .. })
                    && span.source.identifier() == cursor.file
                    && (span.start_offset..=span.start_offset + span.length)
                        .contains(&cursor.offset)
            })
            .min_by_key(|(id, _)| a.ast.exprs.span(*id).length);
        let Some((call, expr)) = call else {
            return Ok(None);
        };
        let (def, skip_self) = match expr {
            Expr::Call { .. } => (a.names.expr(call), 0),
            _ => (a.typed.method(call), 1),
        };
        let Some(DefKind::Func(func)) = def.map(|d| a.names.def(d).kind.clone()) else {
            return Ok(None);
        };

        let parameters = params_of(a, func)
            .into_iter()
            .skip(skip_self)
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p),
                documentation: None,
            })
            .collect();

        // The active parameter: commas between the opening parenthesis and the cursor.
        let span = a.ast.exprs.span(call);
        let before_cursor =
            &cursor.text[span.start_offset..cursor.offset.min(span.start_offset + span.length)];
        let active = before_cursor.find('(').map_or(0, |open| {
            before_cursor[open + 1..].matches(',').count() as u32
        });

        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: signature(a, func),
                documentation: None,
                parameters: Some(parameters),
                active_parameter: Some(active),
            }],
            active_signature: Some(0),
            active_parameter: Some(active),
        }))
    }
}

// --- Describing definitions ---

fn ty(a: &Analysis, t: som_typeck::TypeId) -> String {
    a.typed.display(t, &a.names)
}

/// A definition as it would be declared in som.
fn describe(a: &Analysis, def: DefId) -> String {
    let d = a.names.def(def);
    match &d.kind {
        DefKind::Func(f) => signature(a, *f),
        DefKind::Struct(s) => struct_text(a, *s),
        DefKind::Param { .. } | DefKind::Local(_) => match a.typed.try_local(def) {
            Some(t) => format!("{}: {}", d.name, ty(a, t)),
            None => d.name.to_string(),
        },
        DefKind::TypeParam { .. } => format!("type parameter {}", d.name),
    }
}

fn params_of(a: &Analysis, f: Id<Func>) -> Vec<String> {
    a.ast.funcs[f]
        .params
        .iter()
        .zip(&a.typed.func(f).params)
        .map(|(p, &t)| format!("{}: {}", p.name, ty(a, t)))
        .collect()
}

/// `fn name<T>(a: i32, b: T) -> i32`, or `extern fn ...` for externs.
fn signature(a: &Analysis, f: Id<Func>) -> String {
    let func = &a.ast.funcs[f];
    let sig = a.typed.func(f);
    let generics = if func.type_params.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = func.type_params.iter().map(|p| &*p.name).collect();
        format!("<{}>", names.join(", "))
    };
    let ret = if matches!(a.typed.ty(sig.ret), Ty::Unit) {
        String::new()
    } else {
        format!(" -> {}", ty(a, sig.ret))
    };
    let prefix = if func.body.is_none() { "extern " } else { "" };
    let mut params = params_of(a, f);
    if func.variadic {
        params.push("...".into());
    }
    format!(
        "{prefix}fn {}{generics}({}){ret}",
        func.name,
        params.join(", ")
    )
}

fn struct_text(a: &Analysis, s: Id<Struct>) -> String {
    let def = &a.ast.structs[s];
    let fields: Vec<String> = def
        .fields
        .iter()
        .zip(a.typed.fields(s))
        .map(|(f, &t)| format!("    {}: {},", f.name, ty(a, t)))
        .collect();
    format!("struct {} {{\n{}\n}}", def.name, fields.join("\n"))
}

/// The struct a value of type `t` is, looking through one reference.
fn struct_of(a: &Analysis, t: som_typeck::TypeId) -> Option<DefId> {
    let t = match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    match a.typed.ty(t) {
        Ty::Struct(def) => Some(*def),
        _ => None,
    }
}

/// Where the field read by `object.field` is declared.
fn field_definition(a: &Analysis, object: Id<Expr>, index: Option<usize>) -> Option<Span> {
    let owner = struct_of(a, a.typed.expr(object))?;
    let DefKind::Struct(s) = a.names.def(owner).kind else {
        return None;
    };
    Some(a.ast.structs[s].fields.get(index?)?.name.span.clone())
}

fn location(span: &Span) -> Option<Location> {
    Some(Location {
        uri: Url::from_file_path(span.source.identifier()).ok()?,
        range: convert::span_to_range(span),
    })
}

fn symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    span: &Span,
    name_span: &Span,
    children: Option<Vec<DocumentSymbol>>,
) -> DocumentSymbol {
    #[allow(deprecated)]
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range: convert::span_to_range(span),
        selection_range: convert::span_to_range(name_span),
        children: children.filter(|c| !c.is_empty()),
    }
}

// --- Semantic tokens ---

const TOKEN_TYPES: [SemanticTokenType; 10] = [
    SemanticTokenType::KEYWORD,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::NUMBER,
    SemanticTokenType::STRING,
    SemanticTokenType::TYPE,
    SemanticTokenType::COMMENT,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::STRUCT,
    SemanticTokenType::PARAMETER,
];

/// Token kinds from the lexer, refined with one token of lookaround.
fn semantic_tokens(source: Arc<Source>) -> Vec<SemanticToken> {
    let tokens = lex(source);
    let visible: Vec<&som_lexer::Token> = tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error
            )
        })
        .collect();

    let mut result = Vec::new();
    let (mut prev_line, mut prev_start) = (0, 0);
    for (i, token) in visible.iter().enumerate() {
        let next = visible.get(i + 1).map(|t| t.kind);
        let prev = i.checked_sub(1).map(|p| visible[p].kind);
        let token_type = match token.kind {
            TokenKind::Fn
            | TokenKind::Extern
            | TokenKind::Struct
            | TokenKind::Impl
            | TokenKind::Let
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Mut
            | TokenKind::Use
            | TokenKind::Loop
            | TokenKind::While
            | TokenKind::For => 0,
            TokenKind::Ident if next == Some(TokenKind::OpenParen) => 7,
            TokenKind::Ident if token.text.starts_with(char::is_uppercase) => 8,
            TokenKind::Ident => 1,
            TokenKind::Int | TokenKind::Float | TokenKind::True | TokenKind::False => 2,
            TokenKind::Text => 3,
            TokenKind::I8
            | TokenKind::I16
            | TokenKind::I32
            | TokenKind::I64
            | TokenKind::I128
            | TokenKind::ISize
            | TokenKind::U8
            | TokenKind::U16
            | TokenKind::U32
            | TokenKind::U64
            | TokenKind::U128
            | TokenKind::USize
            | TokenKind::F32
            | TokenKind::F64
            | TokenKind::Bool
            | TokenKind::Char
            | TokenKind::Str => 4,
            TokenKind::Comment => 5,
            // `*` is the pointer type after `:` or `->`, and multiplication or a dereference otherwise.
            TokenKind::Star if matches!(prev, Some(TokenKind::Colon | TokenKind::Arrow)) => 4,
            TokenKind::Star
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Slash
            | TokenKind::Percentage
            | TokenKind::Equals
            | TokenKind::DoubleEquals
            | TokenKind::NotEquals
            | TokenKind::Bang
            | TokenKind::LessThan
            | TokenKind::GreaterThan
            | TokenKind::LessThanOrEqual
            | TokenKind::GreaterThanOrEqual
            | TokenKind::Ampersand => 6,
            _ => continue,
        };

        let line = token.span.start.line.saturating_sub(1) as u32;
        let start = token.span.start.col.saturating_sub(1) as u32;
        let delta_line = line - prev_line;
        result.push(SemanticToken {
            delta_line,
            delta_start: if delta_line == 0 {
                start - prev_start
            } else {
                start
            },
            length: token.span.length as u32,
            token_type,
            token_modifiers_bitset: 0,
        });
        (prev_line, prev_start) = (line, start);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_tokens_classify_calls_types_and_pointers() {
        let source = Arc::new(Source::from_raw("fn f(p: *) -> i32 { g(1) * 2 }"));
        let types: Vec<u32> = semantic_tokens(source)
            .iter()
            .map(|t| t.token_type)
            .collect();
        // fn f p * i32 g 1 * 2   (`(`, `)`, `:`, `->`, `{`, `}` aren't highlighted)
        assert_eq!(types, [0, 7, 1, 4, 4, 7, 2, 6, 2]);
    }
}

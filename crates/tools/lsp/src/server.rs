use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use som_ast::{BinOp, Expr, Func, Id, Item, Span, Stmt, Struct};
use som_base::Source;
use som_driver::Analysis;
use som_lexer::{TokenKind, lex};
use som_resolve::{DefId, DefKind};
use som_typeck::{ArrayMethod, Ty};

use crate::convert;
use crate::index::{AstIndex, NodeRef, items_in_file};

/// The analysis of one project (a directory of `.som` files).
struct Project {
    analysis: Analysis,
    /// Libraries and modules the program could import.
    importable: som_driver::Importable,
    /// Files diagnostics were published for, so they can be cleared next time.
    files: HashSet<String>,
}

pub struct SomLanguageServer {
    client: Client,
    state: Arc<State>,
}

/// What the server knows, shared with analyses running in the background.
#[derive(Default)]
struct State {
    projects: RwLock<HashMap<PathBuf, Project>>,
    /// The open files' text, which may not be saved, by path; and its version.
    documents: RwLock<HashMap<String, (i32, String)>>,
    /// Held while analyzing, so analyses finish in the order they started: an older one
    /// never replaces a newer one's results.
    analyzing: tokio::sync::Mutex<()>,
    /// Whether the editor can be asked to refresh inlay hints and code lenses.
    refresh: std::sync::atomic::AtomicBool,
}

/// How long typing has to pause before the program is analyzed again.
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

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
            state: Arc::default(),
        }
    }

    fn file_of(uri: &Url) -> Option<String> {
        Some(uri.to_file_path().ok()?.to_string_lossy().into_owned())
    }

    /// The text of a file: as it is in the editor if it's open, or else on disk.
    async fn text_of(&self, file: &str) -> Option<String> {
        match self.state.documents.read().await.get(file) {
            Some((_, text)) => Some(text.clone()),
            None => std::fs::read_to_string(file).ok(),
        }
    }

    async fn cursor(&self, uri: &Url, position: &lsp_types::Position) -> Option<Cursor> {
        let file = Self::file_of(uri)?;
        let text = self.text_of(&file).await?;
        let offset = convert::position_to_offset(&text, position)?;
        Some(Cursor { file, text, offset })
    }

    /// The project a file belongs to: the one rooted at its directory, or any project that loaded it.
    fn project_for<'a>(
        projects: &'a HashMap<PathBuf, Project>,
        file: &str,
    ) -> Option<&'a Analysis> {
        Self::project_of(projects, file).map(|p| &p.analysis)
    }

    fn project_of<'a>(projects: &'a HashMap<PathBuf, Project>, file: &str) -> Option<&'a Project> {
        let dir = Path::new(file).parent()?;
        projects.get(dir).or_else(|| {
            projects
                .values()
                .find(|p| p.analysis.ast.modules.iter().any(|m| m.path == dir))
        })
    }

    /// Analyzes the project containing `uri` and publishes its diagnostics.
    async fn analyze(&self, uri: &Url) {
        analyze(&self.client, &self.state, uri).await;
    }

    /// Analyzes again once typing pauses: unless another change comes first.
    fn analyze_soon(&self, uri: Url, version: i32) {
        let (client, state) = (self.client.clone(), self.state.clone());
        tokio::spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            let Some(file) = Self::file_of(&uri) else {
                return;
            };
            let current = state.documents.read().await.get(&file).map(|(v, _)| *v);
            if current == Some(version) {
                analyze(&client, &state, &uri).await;
            }
        });
    }
}

/// Analyzes the project containing `uri`, with the open files' unsaved text, and
/// publishes its diagnostics.
async fn analyze(client: &Client, state: &State, uri: &Url) {
    let _one_at_a_time = state.analyzing.lock().await;
    let Some(root) = uri
        .to_file_path()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let opened = SomLanguageServer::file_of(uri).unwrap_or_default();

    let options = som_driver::Options {
        overlay: (state.documents.read().await.iter())
            .map(|(file, (_, text))| (PathBuf::from(file), text.clone()))
            .collect(),
        ..Default::default()
    };
    let analyzed_root = root.clone();
    let Ok((analysis, importable)) = tokio::task::spawn_blocking(move || {
        let importable = som_driver::importable(&analyzed_root, &options);
        (som_driver::analyze(&analyzed_root, &options), importable)
    })
    .await
    else {
        return;
    };

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

    let mut projects = state.projects.write().await;
    if let Some(previous) = projects.get(&root) {
        for file in previous.files.difference(&files) {
            by_file.entry(file.clone()).or_default();
        }
    }
    for (file, diagnostics) in by_file {
        if let Ok(uri) = Url::from_file_path(&file) {
            client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }
    projects.insert(
        root,
        Project {
            analysis,
            importable,
            files,
        },
    );
    drop(projects);

    // Hints and lenses asked for before this analysis finished are out of date. Each
    // refresh waits for the editor's answer, so it's sent on the side.
    if state.refresh.load(std::sync::atomic::Ordering::Relaxed) {
        let client = client.clone();
        tokio::spawn(async move {
            let _ = client.inlay_hint_refresh().await;
            let _ = client.code_lens_refresh().await;
        });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SomLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let workspace = params.capabilities.workspace.as_ref();
        let refresh = workspace
            .and_then(|w| w.inlay_hint.as_ref())
            .and_then(|i| i.refresh_support)
            .unwrap_or(false);
        (self.state.refresh).store(refresh, std::sync::atomic::Ordering::Relaxed);
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
                declaration_provider: Some(DeclarationCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        "\"".to_string(),
                        " ".to_string(),
                    ]),
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
        let document = params.text_document;
        if let Some(file) = Self::file_of(&document.uri) {
            (self.state.documents.write().await).insert(file, (document.version, document.text));
        }
        self.analyze(&document.uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // From now on it's read from disk; if it wasn't saved, it's analyzed as it is there.
        if let Some(file) = Self::file_of(&params.text_document.uri) {
            self.state.documents.write().await.remove(&file);
        }
        self.analyze(&params.text_document.uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.analyze(&params.text_document.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Full sync: the last change is the whole text.
        let (Some(file), Some(change)) = (
            Self::file_of(&params.text_document.uri),
            params.content_changes.into_iter().last(),
        ) else {
            return;
        };
        let version = params.text_document.version;
        (self.state.documents.write().await).insert(file, (version, change.text));
        self.analyze_soon(params.text_document.uri, version);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = self
            .cursor(&position.text_document.uri, &position.position)
            .await
        else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };
        Ok(
            hover_markdown(a, &cursor.file, cursor.offset).map(|value| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: None,
            }),
        )
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = self
            .cursor(&position.text_document.uri, &position.position)
            .await
        else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };
        if let Some((_, generated)) = extern_links(a, &cursor.file).into_iter().find(|(span, _)| {
            (span.start_offset..=span.start_offset + span.length).contains(&cursor.offset)
        }) {
            return Ok(Url::from_file_path(generated).ok().map(|uri| {
                GotoDefinitionResponse::Scalar(Location {
                    uri,
                    range: Range::default(),
                })
            }));
        }
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
            // In a generated file, a function's definition is its C declaration.
            NodeRef::Func(id) => {
                return Ok(origin_location(a, id).map(GotoDefinitionResponse::Scalar));
            }
            NodeRef::Struct(id) => {
                let location = origin_of(&a.ast.structs[id].name).and_then(location_in);
                return Ok(location.map(GotoDefinitionResponse::Scalar));
            }
            _ => None,
        };

        Ok(target
            .and_then(|span| location(&span))
            .map(GotoDefinitionResponse::Scalar))
    }

    /// For a function imported from C: its declaration in the header.
    async fn goto_declaration(
        &self,
        params: request::GotoDeclarationParams,
    ) -> Result<Option<request::GotoDeclarationResponse>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = self
            .cursor(&position.text_document.uri, &position.position)
            .await
        else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(a) = Self::project_for(&projects, &cursor.file) else {
            return Ok(None);
        };
        let def = match AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset) {
            Some(NodeRef::Func(id)) => origin(a, id),
            Some(NodeRef::Struct(id)) => origin_of(&a.ast.structs[id].name),
            Some(NodeRef::Expr(id)) => {
                let def = match &a.ast.exprs[id] {
                    Expr::MethodCall { .. } => a.typed.method(id),
                    _ => a.names.expr(id),
                };
                def.and_then(|d| def_origin(a, d))
            }
            Some(NodeRef::Type(id)) => a.names.ty(id).and_then(|d| def_origin(a, d)),
            _ => None,
        };
        Ok(def
            .and_then(location_in)
            .map(request::GotoDeclarationResponse::Scalar))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let actions: Vec<CodeActionOrCommand> = params
            .context
            .diagnostics
            .iter()
            .flat_map(convert::fixes_from_json)
            .collect();
        Ok(Some(actions))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let Some(file) = Self::file_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(a) = Self::project_for(&projects, &file) else {
            return Ok(None);
        };
        let mut links: Vec<DocumentLink> = extern_links(a, &file)
            .into_iter()
            .filter_map(|(span, generated)| {
                Some(DocumentLink {
                    range: convert::span_to_range(&span),
                    target: Some(Url::from_file_path(&generated).ok()?),
                    tooltip: Some(format!("Open {}", relative(&generated, &file))),
                    data: None,
                })
            })
            .collect();

        // In a file generated from a header, each function's and struct's name links to
        // its C declaration.
        let origins = som_driver::origins(Path::new(&file));
        let names = (a.ast.funcs.iter().map(|(_, f)| &f.name))
            .chain(a.ast.structs.iter().map(|(_, s)| &s.name));
        for ident in names {
            if ident.span.source.identifier() != file {
                continue;
            }
            let Some((header, line)) = origins.get(&*ident.name) else {
                continue;
            };
            let name = header.file_name().unwrap_or_default().to_string_lossy();
            links.push(DocumentLink {
                range: convert::span_to_range(&ident.span),
                target: header_url(header, *line),
                tooltip: Some(format!("Open {name}:{line}")),
                data: None,
            });
        }
        Ok(Some(links))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some(file) = Self::file_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
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
                    let span = block.methods.last().map_or(block.target.span(), |&m| {
                        block.target.span().merge(ast.funcs.span(m))
                    });
                    symbols.push(symbol(
                        format!("impl {}", block.target),
                        None,
                        SymbolKind::OBJECT,
                        &span,
                        &block.target.span(),
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
        let projects = self.state.projects.read().await;
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
        let Some(cursor) = self
            .cursor(&position.text_document.uri, &position.position)
            .await
        else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(project) = Self::project_of(&projects, &cursor.file) else {
            return Ok(None);
        };
        let a = &project.analysis;
        let insert = Insert::at(&cursor.text, cursor.offset);

        let mut items = Vec::new();
        // `extern ` and `extern "ray` list the libraries som can find, but not ones this
        // file already has an `extern` for, other than the one being edited.
        if let Some(quoted) = extern_library(&cursor.text, cursor.offset) {
            let written: HashSet<&str> = (a.ast.modules.iter())
                .flat_map(|m| &m.items)
                .filter_map(|item| match item {
                    Item::Extern(block) => {
                        Some((block.library.as_deref()?, block.library_span.as_ref()?))
                    }
                    _ => None,
                })
                .filter(|(_, span)| {
                    let end = span.start_offset + span.length;
                    span.source.identifier() == cursor.file
                        && !(span.start_offset..=end).contains(&cursor.offset)
                })
                .map(|(library, _)| library)
                .collect();
            let libraries = (project.importable.libraries.iter())
                .filter(|(name, _)| !written.contains(name.as_str()));
            for (name, path) in libraries {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::MODULE),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: None,
                        description: Some(path.display().to_string()),
                    }),
                    insert_text: (!quoted).then(|| format!("{name:?}")),
                    ..Default::default()
                });
            }
            return Ok(Some(CompletionResponse::Array(items)));
        }
        // A space or quote only asks for the libraries after `extern`.
        let trigger = params.context.and_then(|c| c.trigger_character);
        if matches!(trigger.as_deref(), Some(" " | "\"")) {
            return Ok(None);
        }

        // `alloc(10).` or `alloc(10).si`: after a dot, with maybe part of a name typed.
        let word_start = cursor.text[..cursor.offset]
            .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_')
            .len();
        let before_word = &cursor.text[..word_start];
        let after_dot = before_word.ends_with('.') && !before_word.ends_with("..");

        if after_dot {
            // Fields and methods of the struct before the dot: the outermost expression
            // ending there. A parenthesized one, `(a + b).`, ends before its `)`.
            let dot = word_start - 1;
            let ending_at = |end: usize, parenthesized: bool| {
                a.ast
                    .exprs
                    .iter()
                    .map(|(id, _)| (id, a.ast.exprs.span(id)))
                    .filter(|&(id, span)| {
                        span.source.identifier() == cursor.file
                            && span.start_offset + span.length == end
                            && (parenthesized || can_have_dot(a, id))
                    })
                    .max_by_key(|(_, span)| span.length)
                    .map(|(id, _)| id)
            };
            let mut before = cursor.text[..dot].trim_end();
            let mut parenthesized = false;
            let receiver = loop {
                if let Some(expr) = ending_at(before.len(), parenthesized) {
                    break Some(expr);
                }
                match before.strip_suffix(')') {
                    Some(inner) => (before, parenthesized) = (inner.trim_end(), true),
                    None => break None,
                }
            };
            if let Some(expr) = receiver
                && let Some((element, len)) = array_of(a, a.typed.expr(expr))
            {
                let is_text = matches!(a.typed.ty(a.typed.expr(expr)), Ty::Ref { to, .. }
                    if matches!(a.typed.ty(*to), Ty::Str));
                for (_, method) in ArrayMethod::ALL {
                    // Text can only be read.
                    if is_text && matches!(method, ArrayMethod::GetMut | ArrayMethod::Set) {
                        continue;
                    }
                    let doc = array_method_doc(a, method, element, len);
                    items.push(CompletionItem {
                        label: doc.name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(doc.signature),
                        documentation: markdown(Some(doc.doc)),
                        ..Default::default()
                    });
                }
            } else if let Some(expr) = receiver
                && let Some(owner) = struct_of(a, a.typed.expr(expr))
                && let DefKind::Struct(s) = a.names.def(owner).kind
            {
                for (field, &t) in a.ast.structs[s].fields.iter().zip(a.typed.fields(s)) {
                    items.push(CompletionItem {
                        label: field.name.name.to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(ty(a, t)),
                        documentation: markdown(field.doc.clone()),
                        ..Default::default()
                    });
                }
                for (name, method) in a.names.methods_of(owner) {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(describe(a, method)),
                        documentation: markdown(doc_of(a, method)),
                        ..Default::default()
                    });
                }
            }
        } else if let Some((path, written)) = struct_literal(&cursor.text, cursor.offset) {
            // `Color { r: 1, ` lists the fields not written yet.
            let Some(s) = find_struct(a, path, &cursor.file) else {
                return Ok(None);
            };
            for (field, &t) in a.ast.structs[s].fields.iter().zip(a.typed.fields(s)) {
                if written.contains(&*field.name.name) {
                    continue;
                }
                items.push(CompletionItem {
                    label: field.name.name.to_string(),
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(ty(a, t)),
                    documentation: markdown(field.doc.clone()),
                    insert_text: Some(format!("{}: ", field.name.name)),
                    ..Default::default()
                });
            }
            return Ok(Some(CompletionResponse::Array(items)));
        } else if let Some(module) = qualifier(&cursor.text, cursor.offset) {
            // `raylib::` lists what's in `raylib`.
            if let Some(m) = a.ast.modules.iter().position(|m| *m.name == *module) {
                items.extend(module_items(a, m, None, insert));
            }
            return Ok(Some(CompletionResponse::Array(items)));
        } else {
            // This file's module, and the modules it `use`s, by bare name; other modules by
            // theirs, so `rayl` finds `raylib::init_window`.
            let (here, in_scope) = modules_in_scope(a, &cursor.file);
            for (m, module) in a.ast.modules.iter().enumerate() {
                if in_scope.contains(&m) {
                    items.extend(module_items(a, m, None, insert));
                }
                if m != here && !module.name.is_empty() {
                    items.push(CompletionItem {
                        label: module.name.to_string(),
                        kind: Some(CompletionItemKind::MODULE),
                        ..Default::default()
                    });
                    items.extend(module_items(a, m, Some(&module.name), insert));
                }
            }
            // Modules that aren't imported yet: choosing one adds the import. Libraries are
            // offered in `extern "` instead.
            let loaded = |name: &str| a.ast.modules.iter().any(|m| *m.name == *name);
            let imports = (project.importable.modules.iter())
                .filter(|name| !loaded(name))
                .map(|name| (name, format!("use {name};")));
            for (name, line) in imports {
                items.push(CompletionItem {
                    label: name.clone(),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("add {line}")),
                    insert_text: Some(format!("{name}::")),
                    additional_text_edits: Some(vec![TextEdit {
                        range: Range::default(),
                        new_text: format!("{line}\n"),
                    }]),
                    // Its functions can be listed once the import is saved.
                    command: Some(Command {
                        title: "save".into(),
                        command: "workbench.action.files.save".into(),
                        arguments: None,
                    }),
                    ..Default::default()
                });
            }
            for keyword in [
                "fn", "let", "mut", "if", "else", "struct", "impl", "extern", "loop", "while",
                "break", "use", "out", "true", "false",
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
        let projects = self.state.projects.read().await;
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
            hints.push(type_hint(&name.span, ty(a, t)));
        }

        // What an optional used as a condition asks: `if x` is `if x != none`, and so is each
        // optional operand of `and`, `or` and `!`.
        let mut conditions: Vec<Id<Expr>> = a
            .ast
            .stmts
            .iter()
            .filter_map(|(_, stmt)| match stmt {
                Stmt::If { condition, .. } => Some(*condition),
                _ => None,
            })
            .collect();
        for (_, expr) in a.ast.exprs.iter() {
            match expr {
                Expr::Binary {
                    op: BinOp::And | BinOp::Or,
                    lhs,
                    rhs,
                } => conditions.extend([*lhs, *rhs]),
                Expr::Not(inner) => conditions.push(*inner),
                _ => {}
            }
        }
        conditions.sort_by_key(|e| e.id);
        conditions.dedup();
        for condition in conditions {
            let span = a.ast.exprs.span(condition);
            if span.source.identifier() != file
                || !matches!(a.typed.ty(a.typed.expr(condition)), Ty::Optional(_))
            {
                continue;
            }
            hints.push(InlayHint {
                position: convert::span_to_range(span).end,
                label: InlayHintLabel::String(" != none".into()),
                kind: None,
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String(
                    "`if` on an optional checks whether it holds a value".into(),
                )),
                padding_left: Some(true),
                padding_right: None,
                data: None,
            });
        }

        // The type the parameter gives every `out name` argument: `out db: Sqlite3`.
        for (id, expr) in a.ast.exprs.iter() {
            let Expr::Out(name) = expr else { continue };
            if name.span.source.identifier() != file {
                continue;
            }
            let Some(t) = a.names.expr(id).and_then(|def| a.typed.try_local(def)) else {
                continue;
            };
            if !matches!(a.typed.ty(t), Ty::Error) {
                hints.push(type_hint(&name.span, ty(a, t)));
            }
        }

        // The parameter each argument is for: `init_window(width: 800, …)`.
        for (id, expr) in a.ast.exprs.iter() {
            let (def, args, skip) = match expr {
                Expr::Call { args, .. } => (a.names.expr(id), args, 0),
                // A method's first parameter is the receiver, before the dot.
                Expr::MethodCall { args, .. } => (a.typed.method(id), args, 1),
                _ => continue,
            };
            let Some(DefKind::Func(f)) = def.map(|d| a.names.def(d).kind.clone()) else {
                continue;
            };
            for (&arg, param) in args.iter().zip(a.ast.funcs[f].params.iter().skip(skip)) {
                let span = a.ast.exprs.span(arg);
                if span.source.identifier() != file || !worth_naming(a, arg, &param.name.name) {
                    continue;
                }
                hints.push(InlayHint {
                    position: convert::span_to_range(span).start,
                    label: InlayHintLabel::String(format!("{}:", param.name.name)),
                    kind: Some(InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: None,
                    padding_left: None,
                    padding_right: Some(true),
                    data: None,
                });
            }
        }
        Ok(Some(hints))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let position = &params.text_document_position_params;
        let Some(cursor) = self
            .cursor(&position.text_document.uri, &position.position)
            .await
        else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
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
        // The active parameter: commas between the opening parenthesis and the cursor.
        let span = a.ast.exprs.span(call);
        let before_cursor =
            &cursor.text[span.start_offset..cursor.offset.min(span.start_offset + span.length)];
        let active = before_cursor.find('(').map_or(0, |open| {
            before_cursor[open + 1..].matches(',').count() as u32
        });

        // The label, its parameters, and its documentation.
        let (label, params, doc) = match expr {
            Expr::MethodCall { receiver, .. }
                if let Some(method) = a.typed.array_method(call)
                    && let Some((element, len)) = array_of(a, a.typed.expr(*receiver)) =>
            {
                let doc = array_method_doc(a, method, element, len);
                (doc.signature, doc.params, Some(doc.doc))
            }
            _ => {
                let (def, skip_self) = match expr {
                    Expr::Call { .. } => (a.names.expr(call), 0),
                    _ => (a.typed.method(call), 1),
                };
                let Some(DefKind::Func(func)) = def.map(|d| a.names.def(d).kind.clone()) else {
                    return Ok(None);
                };
                let params = params_of(a, func).into_iter().skip(skip_self).collect();
                (signature(a, func), params, a.ast.funcs[func].doc.clone())
            }
        };
        let parameters = params
            .into_iter()
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p),
                documentation: None,
            })
            .collect();

        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: markdown(doc),
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
        DefKind::Param { func, index } if a.ast.funcs[*func].params[*index].out => {
            let param = &a.ast.funcs[*func].params[*index];
            match a.typed.try_local(def) {
                Some(t) => param_text(a, param, t),
                None => format!("out {}", d.name),
            }
        }
        DefKind::Param { .. } | DefKind::Local(_) | DefKind::Out(_) => match a.typed.try_local(def)
        {
            Some(t) => format!("{}: {}", d.name, ty(a, t)),
            None => d.name.to_string(),
        },
        DefKind::TypeParam { .. } => format!("type parameter {}", d.name),
    }
}

/// What hovering at `offset` in `file` shows: the thing under the cursor as som, and its
/// documentation below it.
fn hover_markdown(a: &Analysis, file: &str, offset: usize) -> Option<String> {
    if let Some(block) = extern_at(a, file, offset) {
        return Some(extern_hover(a, block));
    }
    let node = AstIndex::build(&a.ast).find_at(file, offset)?;

    // What's under the cursor, as som, and its documentation.
    let (text, doc) = match node {
        // Inside `if x { … }`, an optional `x` is the value it holds.
        NodeRef::Expr(id) if a.typed.is_narrowed(id) => {
            let name = match &a.ast.exprs[id] {
                Expr::Var(path) => format!("{path}"),
                Expr::Field { field, .. } => format!(".{}", field.name),
                _ => String::new(),
            };
            let ty = ty(a, a.typed.expr(id));
            let text = if name.is_empty() {
                ty
            } else {
                format!("{name}: {ty}")
            };
            (Some(text), None)
        }
        NodeRef::Expr(id) => match &a.ast.exprs[id] {
            Expr::Var(_) | Expr::Out(_) | Expr::Call { .. } | Expr::Construct { .. } => {
                match a.names.expr(id) {
                    Some(def) => (Some(describe(a, def)), doc_of(a, def)),
                    None => (None, None),
                }
            }
            Expr::MethodCall { receiver, .. } => match a.typed.method(id) {
                Some(def) => (Some(describe(a, def)), doc_of(a, def)),
                None => match (
                    a.typed.array_method(id),
                    array_of(a, a.typed.expr(*receiver)),
                ) {
                    (Some(method), Some((element, len))) => {
                        let doc = array_method_doc(a, method, element, len);
                        (Some(doc.signature), Some(doc.doc))
                    }
                    _ => (None, None),
                },
            },
            Expr::Field { object, field } => (
                Some(format!(".{}: {}", field.name, ty(a, a.typed.expr(id)))),
                field_doc(a, *object, a.typed.field(id)),
            ),
            _ => (Some(ty(a, a.typed.expr(id))), None),
        },
        NodeRef::Type(id) => match a.names.ty(id) {
            Some(def) => (Some(describe(a, def)), doc_of(a, def)),
            None => (None, None),
        },
        NodeRef::Func(id) => (Some(signature(a, id)), func_doc(a, id)),
        NodeRef::Struct(id) => (Some(struct_text(a, id)), struct_doc(a, id)),
        NodeRef::Stmt(id) => match &a.ast.stmts[id] {
            Stmt::Let { .. } => (a.names.local(id).map(|def| describe(a, def)), None),
            _ => (None, None),
        },
    };

    let text = text.filter(|t| !t.contains("{error}"))?;
    Some(match doc {
        Some(doc) => format!("```som\n{text}\n```\n\n{doc}"),
        None => format!("```som\n{text}\n```"),
    })
}

/// A definition's `///` documentation, for functions and structs.
fn doc_of(a: &Analysis, def: DefId) -> Option<String> {
    match a.names.def(def).kind {
        DefKind::Func(f) => func_doc(a, f),
        DefKind::Struct(s) => struct_doc(a, s),
        _ => None,
    }
}

/// A function's documentation, and for one imported from C, a link to its declaration.
fn func_doc(a: &Analysis, f: Id<Func>) -> Option<String> {
    with_origin_link(a.ast.funcs[f].doc.clone(), origin(a, f))
}

/// Documentation followed by a link to where it's declared in C: `[raylib.h:968](…)`.
fn with_origin_link(doc: Option<String>, origin: Option<(PathBuf, usize)>) -> Option<String> {
    let Some((header, line)) = origin else {
        return doc;
    };
    let name = header.file_name()?.to_string_lossy();
    let link = format!("[{name}:{line}]({})", header_url(&header, line)?);
    Some(match doc {
        Some(doc) => format!("{doc}\n\n{link}"),
        None => link,
    })
}

/// Where a function imported from a C header is declared: the header and line.
fn origin(a: &Analysis, f: Id<Func>) -> Option<(PathBuf, usize)> {
    origin_of(&a.ast.funcs[f].name)
}

/// Where something named `name` in a file generated from a C header (a function or a
/// struct) is declared in it.
fn origin_of(name: &som_ast::Ident) -> Option<(PathBuf, usize)> {
    let generated = PathBuf::from(name.span.source.identifier());
    let in_som_folder = generated.parent()?.file_name()? == ".som";
    in_som_folder.then_some(())?;
    som_driver::origins(&generated).remove(&*name.name)
}

/// A struct's documentation, and for one imported from C, a link to its definition.
fn struct_doc(a: &Analysis, s: Id<Struct>) -> Option<String> {
    let doc = a.ast.structs[s].doc.clone();
    with_origin_link(doc, origin_of(&a.ast.structs[s].name))
}

/// Where a function or struct imported from C is declared.
fn def_origin(a: &Analysis, def: DefId) -> Option<(PathBuf, usize)> {
    match a.names.def(def).kind {
        DefKind::Func(f) => origin(a, f),
        DefKind::Struct(s) => origin_of(&a.ast.structs[s].name),
        _ => None,
    }
}

/// A link to a line in a header: `file:///…/raylib.h#L968`.
fn header_url(header: &Path, line: usize) -> Option<Url> {
    let mut url = Url::from_file_path(header).ok()?;
    url.set_fragment(Some(&format!("L{line}")));
    Some(url)
}

/// The location of a function's C declaration, for going to it.
fn origin_location(a: &Analysis, f: Id<Func>) -> Option<Location> {
    location_in(origin(a, f)?)
}

fn location_in((header, line): (PathBuf, usize)) -> Option<Location> {
    let position = lsp_types::Position::new(line.saturating_sub(1) as u32, 0);
    Some(Location {
        uri: Url::from_file_path(header).ok()?,
        range: Range::new(position, position),
    })
}

/// The documentation of the field read by `object.field`.
fn field_doc(a: &Analysis, object: Id<Expr>, index: Option<usize>) -> Option<String> {
    let owner = struct_of(a, a.typed.expr(object))?;
    let DefKind::Struct(s) = a.names.def(owner).kind else {
        return None;
    };
    a.ast.structs[s].fields.get(index?)?.doc.clone()
}

/// Markdown documentation for completion items and signature help.
fn markdown(doc: Option<String>) -> Option<Documentation> {
    doc.map(|value| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        })
    })
}

fn params_of(a: &Analysis, f: Id<Func>) -> Vec<String> {
    a.ast.funcs[f]
        .params
        .iter()
        .zip(&a.typed.func(f).params)
        .map(|(p, &t)| param_text(a, p, t))
        .collect()
}

/// `width: i32`, or `out remainder: i32` for a parameter whose type is `&mut i32`
/// underneath.
fn param_text(a: &Analysis, p: &som_ast::Param, t: som_typeck::TypeId) -> String {
    match (p.out, a.typed.ty(t)) {
        (true, Ty::Ref { to, .. }) => format!("out {}: {}", p.name, ty(a, *to)),
        _ => format!("{}: {}", p.name, ty(a, t)),
    }
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
/// The element type of the array or slice `t` is, looking through one reference, and its
/// length if that's fixed.
fn array_of(a: &Analysis, t: som_typeck::TypeId) -> Option<(som_typeck::TypeId, Option<u64>)> {
    let t = match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    match a.typed.ty(t) {
        Ty::Array { element, len } => Some((*element, Some(*len))),
        Ty::Slice(element) => Some((*element, None)),
        // Text is its bytes.
        Ty::Str => Some((a.typed.table.u8, None)),
        _ => None,
    }
}

/// One of the methods every array has, as the editor shows it.
struct ArrayMethodDoc {
    name: &'static str,
    signature: String,
    params: Vec<String>,
    doc: String,
}

/// How `method` reads for an array or slice of elements of type `element`, with `len` of them
/// if that's fixed.
fn array_method_doc(
    a: &Analysis,
    method: ArrayMethod,
    element: som_typeck::TypeId,
    len: Option<u64>,
) -> ArrayMethodDoc {
    let t = ty(a, element);
    let index = "index: any integer".to_string();
    let bounds = "or `none` if `index` is out of bounds.";
    let (name, params, returns, doc) = match method {
        ArrayMethod::Len => (
            "len",
            vec![],
            "any integer".to_string(),
            match len {
                Some(len) => {
                    format!("The number of elements: {len}. It works as any integer type.")
                }
                None => "The number of elements. It works as any integer type.".to_string(),
            },
        ),
        ArrayMethod::Get => (
            "get",
            vec![index],
            format!("{t}?"),
            format!("A copy of the element at `index`, {bounds}"),
        ),
        ArrayMethod::GetRef => (
            "get_ref",
            vec![index],
            format!("&{t}?"),
            format!("A reference to the element at `index`, {bounds}"),
        ),
        ArrayMethod::GetMut => (
            "get_mut",
            vec![index],
            format!("&mut {t}?"),
            format!("A mutable reference to the element at `index`, {bounds}"),
        ),
        ArrayMethod::Set => (
            "set",
            vec![index, format!("value: {t}")],
            "bool".to_string(),
            "Stores `value` at `index`, and returns whether `index` was in bounds. If it \
             wasn't, nothing changes."
                .to_string(),
        ),
    };
    ArrayMethodDoc {
        name,
        signature: format!("fn {name}({}) -> {returns}", params.join(", ")),
        params,
        doc,
    }
}

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

/// The `extern` whose library or header name is at `offset` in `file`.
fn extern_at<'a>(a: &'a Analysis, file: &str, offset: usize) -> Option<&'a som_ast::Extern> {
    a.ast
        .modules
        .iter()
        .flat_map(|m| &m.items)
        .find_map(|item| {
            let Item::Extern(block) = item else {
                return None;
            };
            let header = block.header.as_ref().map(|(_, span)| span);
            let at = block.library_span.iter().chain(header).any(|span| {
                span.source.identifier() == file
                    && (span.start_offset..=span.start_offset + span.length).contains(&offset)
            });
            at.then_some(block)
        })
}

/// What an `extern "raylib";` brings in: where the library and header are, how much was
/// imported, and what was skipped and why.
fn extern_hover(a: &Analysis, block: &som_ast::Extern) -> String {
    let header = block.header.as_ref().map(|(h, _)| h.as_str());
    // `extern "raylib";` implies its header, at the library's name; show it as written.
    let written = block
        .header
        .as_ref()
        .filter(|(_, span)| Some(span) != block.library_span.as_ref())
        .map(|(h, _)| h.as_str());
    let mut text = match (&block.library, written) {
        (Some(library), Some(header)) => {
            format!("```som\nextern {library:?} from {header:?}\n```\n")
        }
        (Some(library), None) => format!("```som\nextern {library:?}\n```\n"),
        (None, Some(header)) => format!("```som\nextern from {header:?}\n```\n"),
        (None, None) => "```som\nextern\n```\n".to_string(),
    };
    let root = &a.ast.modules[0].path;
    let (library_file, header_file) =
        som_driver::locate(block.library.as_deref(), header, root, &Default::default());
    let found = |file: Option<PathBuf>| match file {
        Some(file) => format!("`{}`", file.display()),
        None => "not found".to_string(),
    };
    if block.library.is_some() {
        text.push_str(&format!("\n**library** {}  ", found(library_file)));
    }
    text.push_str(&format!("\n**header** {}", found(header_file)));

    // The som generated from the header: what's in it, and what it says it skipped.
    let Some(module) = block.module.map(|m| &a.ast.modules[m]) else {
        return text;
    };
    let Some(generated) = module.files.first() else {
        return text;
    };
    let mut funcs = 0;
    let mut structs = 0;
    for item in &module.items {
        match item {
            Item::Struct(_) => structs += 1,
            Item::Extern(b) => funcs += b.funcs.len(),
            Item::Func(_) => funcs += 1,
            _ => {}
        }
    }
    let skipped: Vec<&str> = generated
        .content()
        .lines()
        .filter_map(|line| line.strip_prefix("// skipped "))
        .collect();
    text.push_str(&format!(
        "\n\n**imported** {funcs} functions and {structs} structs into `{}`",
        module.name
    ));
    if !skipped.is_empty() {
        text.push_str(&format!("\n\n**skipped** {}:\n", skipped.len()));
        const SHOWN: usize = 8;
        for line in skipped.iter().take(SHOWN) {
            let (name, reason) = line.split_once(": ").unwrap_or((line, ""));
            text.push_str(&format!("\n- `{name}`: {reason}"));
        }
        if skipped.len() > SHOWN {
            text.push_str(&format!(
                "\n- and {} more, listed in `{}`",
                skipped.len() - SHOWN,
                generated.identifier()
            ));
        }
    }
    text
}

/// `path` from the directory of `file`: `.som/raylib.som`.
fn relative(path: &Path, file: &str) -> String {
    Path::new(file)
        .parent()
        .and_then(|dir| path.strip_prefix(dir).ok())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The names in `extern "raylib";` and `from "raylib.h"` in `file`, each with the som
/// generated from the header that it imports (`.som/raylib.som`).
fn extern_links(a: &Analysis, file: &str) -> Vec<(Span, PathBuf)> {
    let mut links = Vec::new();
    for item in a.ast.modules.iter().flat_map(|m| &m.items) {
        let Item::Extern(block) = item else { continue };
        let generated = block
            .module
            .and_then(|m| a.ast.modules[m].files.first())
            .map(|source| PathBuf::from(source.identifier()));
        let Some(generated) = generated else { continue };
        let header = block.header.as_ref().map(|(_, span)| span);
        for span in block.library_span.iter().chain(header) {
            // In `extern "raylib";` the header is implied, at the library's name.
            let seen = links
                .iter()
                .any(|(s, _): &(Span, _)| s.start_offset == span.start_offset);
            if span.source.identifier() == file && !seen {
                links.push((span.clone(), generated.clone()));
            }
        }
    }
    links
}

/// The module `file` is in, and the modules whose names it can use bare: its own and
/// the ones it `use`s.
fn modules_in_scope(a: &Analysis, file: &str) -> (usize, Vec<usize>) {
    let here = a
        .ast
        .modules
        .iter()
        .position(|m| m.files.iter().any(|f| f.identifier() == file))
        .unwrap_or(0);
    let mut in_scope = vec![here];
    for item in &a.ast.modules[here].items {
        if let Item::Use(u) = item
            && let [name] = u.path.segments.as_slice()
            && let Some(m) = a.ast.modules.iter().position(|m| *m.name == *name.name)
        {
            in_scope.push(m);
        }
    }
    (here, in_scope)
}

/// The struct a literal names: `raylib::Color` in that module, `Color` in the modules
/// in scope.
fn find_struct(a: &Analysis, path: &str, file: &str) -> Option<Id<Struct>> {
    let modules = match path.rsplit_once("::") {
        Some((module, _)) => a
            .ast
            .modules
            .iter()
            .position(|m| *m.name == *module)
            .into_iter()
            .collect(),
        None => modules_in_scope(a, file).1,
    };
    let name = path.rsplit("::").next().unwrap_or(path);
    modules.into_iter().find_map(|m| {
        a.ast.modules[m].items.iter().find_map(|item| match item {
            Item::Struct(id) if *a.ast.structs[*id].name.name == *name => Some(*id),
            _ => None,
        })
    })
}

/// When the cursor is where a field's name goes in a struct literal, `Color { r: 1, |`:
/// the struct's path, and the fields already written in it (before or after the cursor).
fn struct_literal(text: &str, offset: usize) -> Option<(&str, HashSet<&str>)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';

    // The `{` the cursor is in, looking back past nested brackets.
    let mut depth = 0;
    let mut open = None;
    for (i, c) in text.get(..offset)?.char_indices().rev() {
        match c {
            '}' | ')' | ']' => depth += 1,
            '{' | '(' | '[' if depth > 0 => depth -= 1,
            '{' => {
                open = Some(i);
                break;
            }
            '(' | '[' | ';' => return None,
            _ => {}
        }
    }
    let open = open?;

    // A type's path right before it, and not `struct Color {` or `impl Color {`.
    let before = text[..open].trim_end();
    let start = before
        .rfind(|c: char| !(word(c) || c == ':'))
        .map_or(0, |i| i + 1);
    let path = before[start..].trim_start_matches(':');
    let first = path.chars().next()?;
    if !(first.is_alphabetic() || first == '_') {
        return None;
    }
    let keyword = before[..start]
        .trim_end()
        .rsplit(|c: char| !word(c))
        .next()
        .unwrap_or_default();
    if matches!(keyword, "struct" | "impl" | "fn") {
        return None;
    }

    // The literal's fields, split at commas outside nested brackets, up to its `}`.
    let mut fields = Vec::new();
    let (mut depth, mut from, mut end) = (0, open + 1, text.len());
    for (i, c) in text[open + 1..].char_indices() {
        let i = i + open + 1;
        match c {
            '{' | '(' | '[' => depth += 1,
            '}' if depth == 0 => {
                end = i;
                break;
            }
            '}' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                fields.push(from..i);
                from = i + 1;
            }
            _ => {}
        }
    }
    fields.push(from..end);

    let mut written = HashSet::new();
    for field in fields {
        if field.contains(&offset) || field.end == offset {
            // After `name:` comes a value, not a field.
            if field_name(&text[field.start..offset]).is_some() {
                return None;
            }
        } else if let Some(name) = field_name(&text[field]) {
            written.insert(name);
        }
    }
    Some((path, written))
}

/// The name in a struct literal's `name: value`.
fn field_name(field: &str) -> Option<&str> {
    let field = field.trim_start();
    let len = field
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(field.len());
    let rest = field[len..].trim_start();
    (len > 0 && rest.starts_with(':') && !rest.starts_with("::")).then(|| &field[..len])
}

/// Whether the cursor is where `extern`'s library name goes: in its string, `extern "ray`
/// (`Some(true)`), or before the string is opened, `extern ray` (`Some(false)`).
fn extern_library(text: &str, offset: usize) -> Option<bool> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let after_extern = |before: &str| {
        before
            .strip_suffix("extern")
            .is_some_and(|rest| !rest.ends_with(word))
    };
    let before = text.get(..offset)?;
    if let Some(quote) = before.rfind('"')
        && !before[quote + 1..].contains('\n')
        && after_extern(before[..quote].trim_end())
    {
        return Some(true);
    }
    let rest = before.trim_end_matches(word);
    let spaced = rest.ends_with(char::is_whitespace);
    (spaced && after_extern(rest.trim_end())).then_some(false)
}

/// The module named right before the word being typed: `raylib` in `raylib::ini`.
fn qualifier(text: &str, offset: usize) -> Option<&str> {
    let before = text.get(..offset)?;
    let word = before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    let module = word.strip_suffix("::")?;
    let start = module
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    Some(&module[start..]).filter(|m| !m.is_empty())
}

/// Completions for the functions and structs in module `m`, named `module::name` when a
/// module is given.
fn module_items(
    a: &Analysis,
    m: usize,
    module: Option<&str>,
    insert: Insert,
) -> Vec<CompletionItem> {
    let name = |n: &str| match module {
        Some(module) => format!("{module}::{n}"),
        None => n.to_string(),
    };
    let mut funcs: Vec<Id<Func>> = Vec::new();
    let mut items = Vec::new();
    for item in &a.ast.modules[m].items {
        match item {
            Item::Func(id) => funcs.push(*id),
            Item::Extern(block) if block.module.is_none_or(|b| b == m) => {
                funcs.extend(&block.funcs)
            }
            Item::Struct(id) => items.push(struct_item(
                a,
                *id,
                name(&a.ast.structs[*id].name.name),
                insert,
            )),
            _ => {}
        }
    }
    // Hand-written `extern "raylib" { … }` blocks elsewhere put their functions here too.
    for (index, other) in a.ast.modules.iter().enumerate() {
        if index == m {
            continue;
        }
        for item in &other.items {
            if let Item::Extern(block) = item
                && block.module == Some(m)
            {
                funcs.extend(&block.funcs);
            }
        }
    }
    for f in funcs {
        let func = &a.ast.funcs[f];
        if *func.name.name == *"main" && module.is_none() && m == 0 {
            continue;
        }
        items.push(func_item(a, f, name(&func.name.name), insert));
    }
    items
}

/// Whether `expr` can be what's before a `.` without parentheses: not `a + b` (the dot
/// goes with `b`), and not a call missing its `)` (`free(alloc(10).`: the dot goes with
/// the argument).
fn can_have_dot(a: &Analysis, expr: Id<Expr>) -> bool {
    let end = |id: Id<Expr>| {
        let span = a.ast.exprs.span(id);
        span.start_offset + span.length
    };
    match &a.ast.exprs[expr] {
        Expr::Binary { .. }
        | Expr::Not(_)
        | Expr::Borrow { .. }
        | Expr::Deref(_)
        | Expr::Assign { .. }
        | Expr::Conditional { .. } => false,
        Expr::Call { args, .. } | Expr::MethodCall { args, .. } => {
            args.last().is_none_or(|&last| end(last) != end(expr))
        }
        _ => true,
    }
}

/// `: i32` after the name a variable is declared with.
fn type_hint(name: &Span, ty: String) -> InlayHint {
    InlayHint {
        position: convert::span_to_range(name).end,
        label: InlayHintLabel::String(format!(": {ty}")),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: None,
        padding_left: None,
        padding_right: None,
        data: None,
    }
}

/// Whether an argument's parameter name says something: not when the argument is a
/// variable of that name already (`width` for `width`), or the name was made up because
/// the C header had none (`arg0`).
fn worth_naming(a: &Analysis, arg: Id<Expr>, param: &str) -> bool {
    let made_up = param
        .strip_prefix("arg")
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let same = match &a.ast.exprs[arg] {
        Expr::Var(path) => *path.name().name == *param,
        Expr::Out(name) => *name.name == *param,
        _ => false,
    };
    !made_up && !same
}

/// Where a completion goes, which decides whether it's a snippet.
#[derive(Clone, Copy)]
struct Insert {
    /// A value goes here, not a type: a struct completes to a literal.
    value: bool,
    /// The text after the word already opens a call: `init_win|(800, …)`.
    call_follows: bool,
}

impl Insert {
    fn at(text: &str, offset: usize) -> Insert {
        let after = text[offset..].trim_start_matches(|c: char| c.is_alphanumeric() || c == '_');
        Insert {
            value: !type_position(text, offset),
            call_follows: after.starts_with('('),
        }
    }
}

/// `init_window(${1:width}, ${2:height}, ${3:title})`: the call, with a tab stop on each
/// argument.
fn func_item(a: &Analysis, f: Id<Func>, label: String, insert: Insert) -> CompletionItem {
    let func = &a.ast.funcs[f];
    let snippet = (!insert.call_follows).then(|| {
        let args: Vec<String> = func
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let out = if p.out { "out " } else { "" };
                format!("{out}${{{}:{}}}", i + 1, p.name.name)
            })
            .collect();
        format!("{label}({}){}", args.join(", "), "$0")
    });
    CompletionItem {
        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
        insert_text: snippet,
        label,
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(signature(a, f)),
        documentation: markdown(func.doc.clone()),
        ..Default::default()
    }
}

/// `Color { r: $1, g: $2, b: $3, a: $4 }` where a value goes; just the name in a type.
/// A struct with many fields gets empty braces, and field completion inside them.
fn struct_item(a: &Analysis, s: Id<Struct>, label: String, insert: Insert) -> CompletionItem {
    let st = &a.ast.structs[s];
    let snippet = insert.value.then(|| {
        if st.fields.is_empty() {
            format!("{label} {{}}$0")
        } else if st.fields.len() > 6 {
            format!("{label} {{ $0 }}")
        } else {
            let fields: Vec<String> = st
                .fields
                .iter()
                .enumerate()
                .map(|(i, field)| format!("{}: ${}", field.name.name, i + 1))
                .collect();
            format!("{label} {{ {} }}$0", fields.join(", "))
        }
    });
    CompletionItem {
        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
        insert_text: snippet,
        label,
        kind: Some(CompletionItemKind::STRUCT),
        documentation: markdown(st.doc.clone()),
        ..Default::default()
    }
}

/// Whether a type goes at `offset`: after `->`, or after the `:` of a `let`, a function
/// parameter or a struct's field definition (not a struct literal's field value).
fn type_position(text: &str, offset: usize) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    // Past the word being typed and its qualifier: `raylib::Col`.
    let mut before = text[..offset].trim_end_matches(word);
    while let Some(rest) = before.strip_suffix("::") {
        before = rest.trim_end_matches(word);
    }
    let before = before.trim_end().trim_end_matches('&').trim_end();
    let before = before
        .strip_suffix("mut")
        .filter(|b| !b.ends_with(word))
        .unwrap_or(before)
        .trim_end();
    if before.ends_with("->") {
        return true;
    }
    let Some(before) = before.strip_suffix(':').filter(|b| !b.ends_with(':')) else {
        return false;
    };
    // `let x:`, `let mut x:`
    let named = before.trim_end().trim_end_matches(word).trim_end();
    let keyword = named.rsplit(|c: char| !word(c)).next().unwrap_or_default();
    if matches!(keyword, "let" | "mut") {
        return true;
    }
    // `fn f(a: …` or `struct S { a: …`: what the enclosing bracket belongs to.
    let mut depth = 0;
    for (i, c) in named.char_indices().rev() {
        match c {
            ')' | '}' | ']' => depth += 1,
            '(' | '{' | '[' if depth > 0 => depth -= 1,
            '(' | '{' => {
                // The keyword before the name: `fn name<T>(` or `struct Name {`.
                let name = named[..i].trim_end();
                let name = match name.strip_suffix('>') {
                    Some(generic) => &generic[..generic.rfind('<').unwrap_or(0)],
                    None => name,
                };
                let keyword = name.trim_end_matches(word).trim_end();
                return keyword.ends_with(if c == '(' { "fn" } else { "struct" });
            }
            _ => {}
        }
    }
    false
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
            | TokenKind::Out
            | TokenKind::Use
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Loop
            | TokenKind::While
            | TokenKind::Break
            | TokenKind::For => 0,
            TokenKind::Ident if next == Some(TokenKind::OpenParen) => 7,
            TokenKind::Ident if token.text.starts_with(char::is_uppercase) => 8,
            TokenKind::Ident => 1,
            TokenKind::Int
            | TokenKind::Float
            | TokenKind::True
            | TokenKind::False
            | TokenKind::None => 2,
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
            | TokenKind::Str
            | TokenKind::CStr => 4,
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
            | TokenKind::Ampersand
            | TokenKind::Question => 6,
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

    #[test]
    fn library_names_complete_in_extern_strings() {
        let at_end = |text: &str| extern_library(text, text.len());
        assert_eq!(at_end("extern \""), Some(true));
        assert_eq!(at_end("use std;\nextern \"ray"), Some(true));
        assert_eq!(at_end("extern\"gc"), Some(true));
        assert_eq!(at_end("extern "), Some(false));
        assert_eq!(at_end("use std;\nextern ray"), Some(false));
        assert_eq!(at_end("extern"), None);
        assert_eq!(at_end("extern \"raylib\";\n"), None);
        assert_eq!(at_end("extern \"raylib\" from \"ray"), None);
        assert_eq!(at_end("my_extern \""), None);
        assert_eq!(at_end("my_extern ray"), None);
        assert_eq!(at_end("let s = \"ray"), None);
    }

    #[test]
    fn struct_literals_and_their_written_fields() {
        /// The literal at the end of `text`, and its written fields, sorted.
        fn at_end(text: &str) -> Option<(&str, Vec<&str>)> {
            struct_literal(text, text.len()).map(|(path, written)| {
                let mut written: Vec<&str> = written.into_iter().collect();
                written.sort();
                (path, written)
            })
        }
        assert_eq!(
            at_end("let c = raylib::Color { "),
            Some(("raylib::Color", vec![]))
        );
        assert_eq!(
            at_end("Color { r: 1, g: f(2, 3), "),
            Some(("Color", vec!["g", "r"]))
        );
        assert_eq!(
            at_end("Color { r: Point { x: 1 }, b"),
            Some(("Color", vec!["r"]))
        );
        assert_eq!(at_end("Color { r: "), None, "a value goes here");
        assert_eq!(at_end("Color { r: raylib::"), None);
        assert_eq!(at_end("struct Color { "), None);
        assert_eq!(at_end("fn main() { "), None);
        assert_eq!(at_end("f("), None);

        // Fields after the cursor count too.
        let text = "Color { r: 1, , b: 3 }";
        let (_, written) = struct_literal(text, text.find(", ,").unwrap() + 2).unwrap();
        assert_eq!(written, HashSet::from(["r", "b"]));
    }

    #[test]
    fn types_go_after_arrows_and_annotations_not_in_literals() {
        for (text, is_type) in [
            ("let c: raylib::Col", true),
            ("let mut c: &Col", true),
            ("fn f(a: i32, b: Col", true),
            ("fn f<T>(a: Col", true),
            ("fn f() -> Col", true),
            ("struct Rect { color: Col", true),
            ("let r = Rect { color: Col", false),
            ("let c = raylib::Col", false),
            ("    Col", false),
            ("f(Col", false),
        ] {
            assert_eq!(type_position(text, text.len()), is_type, "{text}");
        }
    }

    #[test]
    fn parameter_hints_name_only_what_needs_naming() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn area(width: i32, height: i32, arg0: i32) -> i32 { width * height }\n\
                      fn main() { let width = 2; area(width, 3, 4); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let named: Vec<&str> = a
            .ast
            .exprs
            .iter()
            .filter_map(|(_, e)| match e {
                Expr::Call { args, .. } => Some(args),
                _ => None,
            })
            .flatten()
            .zip(["width", "height", "arg0"])
            .filter(|(arg, param)| worth_naming(&a, **arg, param))
            .map(|(_, param)| param)
            .collect();
        assert_eq!(named, ["height"]);
    }

    #[test]
    fn out_parameters_show_as_out() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "fn divide(a: i32, b: i32, out remainder: i32) -> i32 {\n    \
                      remainder = a % b;\n    a / b\n}\n\n\
                      fn main() { let q = divide(17, 5, out remainder); }\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let file = main.to_str().unwrap();
        let hover = |needle: &str| hover_markdown(&a, file, source.find(needle).unwrap() + 1);

        let signature = "fn divide(a: i32, b: i32, out remainder: i32) -> i32";
        assert_eq!(
            hover("divide(17").as_deref(),
            Some(format!("```som\n{signature}\n```").as_str())
        );
        // Inside the function, and the variable `out` declares at the call.
        assert_eq!(
            hover("remainder = ").as_deref(),
            Some("```som\nout remainder: i32\n```")
        );
        assert_eq!(
            hover_markdown(&a, file, source.find("out remainder)").unwrap() + 5).as_deref(),
            Some("```som\nremainder: i32\n```")
        );

        let divide = a.ast.funcs.iter().find(|(_, f)| &*f.name.name == "divide");
        let item = func_item(
            &a,
            divide.unwrap().0,
            "divide".into(),
            Insert {
                value: true,
                call_follows: false,
            },
        );
        assert_eq!(
            item.insert_text.as_deref(),
            Some("divide(${1:a}, ${2:b}, out ${3:remainder})$0")
        );

        let out = a.ast.exprs.iter().find(|(_, e)| matches!(e, Expr::Out(_)));
        assert!(!worth_naming(&a, out.unwrap().0, "remainder"));
    }

    #[test]
    fn qualifier_is_the_module_before_the_word() {
        fn at_end(text: &str) -> Option<&str> {
            qualifier(text, text.len())
        }
        assert_eq!(at_end("    raylib::ini"), Some("raylib"));
        assert_eq!(at_end("(raylib::"), Some("raylib"));
        assert_eq!(at_end("    rayl"), None);
        assert_eq!(at_end("::x"), None);
    }

    #[test]
    fn hover_shows_documentation() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main.som");
        let source = "/// A point.\nstruct Point {\n    /// Across.\n    x: i32,\n}\n\n\
                      /// Squares a number.\n/// Twice.\nfn square(n: i32) -> i32 { n * n }\n\n\
                      fn main() {\n    let p = Point { x: 2 };\n    let y = square(p.x);\n}\n";
        std::fs::write(&main, source).unwrap();
        let a = som_driver::analyze(&main, &Default::default());
        let file = main.to_str().unwrap();
        let hover = |needle: &str| hover_markdown(&a, file, source.find(needle).unwrap() + 1);

        assert_eq!(
            hover("square(p").as_deref(),
            Some("```som\nfn square(n: i32) -> i32\n```\n\nSquares a number.\nTwice.")
        );
        assert!(hover("Point {").unwrap().ends_with("```\n\nA point."));
        assert!(
            hover("p.x")
                .map(|_| ())
                .and(hover_markdown(&a, file, source.find("p.x").unwrap() + 2))
                .unwrap()
                .ends_with("```\n\nAcross.")
        );
        assert_eq!(
            hover("main").as_deref(),
            Some("```som\nfn main()\n```"),
            "no documentation, no gap"
        );
    }
}

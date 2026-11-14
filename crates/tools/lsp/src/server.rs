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
use som_resolve::{DefId, DefKind, LangTrait};
use som_typeck::{ArrayMethod, PointerMethod, Ty};

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
        // `break x` is one thing: the hover covers all of it, not just the word.
        let found = AstIndex::build(&a.ast).find_at(&cursor.file, cursor.offset);
        let range = match found.map(|node| hover_node(a, node)) {
            Some(NodeRef::Stmt(id)) => jump_span(a, id).map(|span| convert::span_to_range(&span)),
            // A range is one thing, from any part of it.
            Some(NodeRef::Expr(id)) if matches!(a.ast.exprs[id], Expr::Range { .. }) => {
                Some(convert::span_to_range(a.ast.exprs.span(id)))
            }
            _ => None,
        };
        Ok(
            hover_markdown(a, &cursor.file, cursor.offset).map(|value| Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range,
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
                Expr::Var(_) | Expr::Call { .. } | Expr::Construct { .. } => (a.typed)
                    .callee(id, &a.names)
                    .map(|d| a.names.def(d).span.clone()),
                Expr::MethodCall { .. } => a.typed.method(id).map(|d| a.names.def(d).span.clone()),
                Expr::Field { object, .. } => match a.typed.method(id) {
                    // A property, `list.len`.
                    Some(d) => Some(a.names.def(d).span.clone()),
                    None => field_definition(a, *object, a.typed.field(id)),
                },
                _ => None,
            },
            NodeRef::Type(id) => a.names.ty(id).map(|d| a.names.def(d).span.clone()),
            // `break x` is one thing: the link starts from all of it, so the editor underlines
            // all of it.
            NodeRef::Stmt(id) => {
                let (Some(from), Some(to)) = (jump_span(a, id), loop_of(a, id)) else {
                    return Ok(None);
                };
                let to = loop_header(a, to);
                let Ok(target_uri) = Url::from_file_path(to.source.identifier()) else {
                    return Ok(None);
                };
                let target = convert::span_to_range(&to);
                return Ok(Some(GotoDefinitionResponse::Link(vec![LocationLink {
                    origin_selection_range: Some(convert::span_to_range(&from)),
                    target_uri,
                    target_range: target,
                    target_selection_range: target,
                }])));
            }
            // In a generated file, a function's definition is its C declaration.
            NodeRef::Func(id) => {
                return Ok(origin_location(a, id).map(GotoDefinitionResponse::Scalar));
            }
            NodeRef::Struct(id) => {
                let location = origin_of(&a.ast.structs[id].name).and_then(location_in);
                return Ok(location.map(GotoDefinitionResponse::Scalar));
            }
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
                    Expr::MethodCall { .. } | Expr::Field { .. } => a.typed.method(id),
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
                Item::Trait(id) => {
                    let t = &ast.traits[*id];
                    let methods: Vec<DocumentSymbol> = (t.methods.iter())
                        .map(|&m| func_symbol(m, SymbolKind::METHOD))
                        .collect();
                    symbols.push(symbol(
                        t.name.name.to_string(),
                        None,
                        SymbolKind::INTERFACE,
                        ast.traits.span(*id),
                        &t.name.span,
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
        // The editor's text, not the file on disk: unsaved edits would shift every token.
        let Some(file) = Self::file_of(&params.text_document.uri) else {
            return Ok(None);
        };
        let Some(text) = self.text_of(&file).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens(Arc::new(Source::from_raw_at(text, file))),
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

        // `use som::`: only modules go in a `use`, the next part of their path.
        if let Some(prefix) = use_path(&cursor.text, cursor.offset) {
            // Not what's already in scope: only what's still to `use`, or leads to it.
            let (here, in_scope) = modules_in_scope(a, &cursor.file, cursor.offset);
            let used: HashSet<String> = (in_scope.iter())
                .map(|&m| a.ast.modules[m].name.to_string())
                .collect();
            let names: Vec<String> = (a.ast.modules.iter().enumerate())
                .filter(|&(m, module)| m != here && !module.name.is_empty())
                .map(|(_, module)| module.name.to_string())
                .chain(project.importable.modules.iter().cloned())
                .filter(|name| !used.contains(name))
                .collect();
            let mut seen = HashSet::new();
            for name in &names {
                let Some(rest) = name.strip_prefix(prefix) else {
                    continue;
                };
                let next = rest.split("::").next().unwrap_or_default();
                if next.is_empty() || !seen.insert(next.to_string()) {
                    continue;
                }
                items.push(CompletionItem {
                    label: next.to_string(),
                    kind: Some(CompletionItemKind::MODULE),
                    detail: Some(format!("{prefix}{next}")),
                    ..Default::default()
                });
            }
            return Ok(Some(CompletionResponse::Array(items)));
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
                && let Some(sequence) = array_of(a, a.typed.expr(expr))
            {
                for (_, method) in ArrayMethod::ALL {
                    // Text can only be read, and only text has `bytes`.
                    let skip = if sequence.is_text {
                        matches!(
                            method,
                            ArrayMethod::GetMut | ArrayMethod::Set | ArrayMethod::SliceMut
                        )
                    } else {
                        method == ArrayMethod::Bytes
                    };
                    if skip {
                        continue;
                    }
                    let doc = array_method_doc(a, method, sequence);
                    items.push(CompletionItem {
                        label: doc.name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(doc.signature),
                        documentation: markdown(Some(doc.doc)),
                        ..Default::default()
                    });
                }
            } else if let Some(expr) = receiver
                && let Ty::Ptr(element) = *a.typed.ty(a.typed.expr(expr))
            {
                // A typed pointer's built-in methods.
                for (name, method) in PointerMethod::ALL {
                    let (params, ret, doc) = pointer_method_doc(method);
                    let element = ty(a, element);
                    let fill = |s: &str| s.replace('T', &element);
                    let signature = format!(
                        "fn {name}({}){}",
                        params
                            .iter()
                            .map(|(p, t)| format!("{p}: {}", fill(t)))
                            .collect::<Vec<_>>()
                            .join(", "),
                        if ret.is_empty() {
                            String::new()
                        } else {
                            format!(" -> {}", fill(ret))
                        }
                    );
                    let snippet = (!insert.call_follows).then(|| {
                        let args: Vec<String> = (params.iter().enumerate())
                            .map(|(i, (p, _))| format!("${{{}:{p}}}", i + 1))
                            .collect();
                        format!("{name}({})$0", args.join(", "))
                    });
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(signature),
                        documentation: markdown(Some(doc.to_string())),
                        ..Default::default()
                    });
                }
            } else if let Some(expr) = receiver
                && let Some(owner) = struct_of(a, a.typed.expr(expr))
                && let DefKind::Struct(s) = a.names.def(owner).kind
            {
                // A generic struct's fields as they are here: `i32`, not `T`.
                let subst = struct_subst(a, s, a.typed.expr(expr));
                for (field, t) in readable_fields(a, s) {
                    items.push(CompletionItem {
                        label: field.name.name.to_string(),
                        kind: Some(CompletionItemKind::FIELD),
                        detail: Some(a.typed.table.display_with(t, &a.names, &subst)),
                        documentation: markdown(field.doc.clone()),
                        ..Default::default()
                    });
                }
                for (name, method) in a.names.methods_of(owner) {
                    // As it is on this receiver: `Vector<i32>`, not `Vector<T>`.
                    let detail = match a.names.def(method).kind {
                        DefKind::Func(f) => {
                            signature_with(a, f, &method_subst(a, f, a.typed.expr(expr)))
                        }
                        _ => describe(a, method),
                    };
                    // `v.push(${1:value})`: the receiver is `self`.
                    let snippet = match a.names.def(method).kind {
                        DefKind::Func(f) => call_snippet(a, f, name, 1, insert),
                        _ => None,
                    };
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(detail),
                        documentation: markdown(doc_of(a, method)),
                        ..Default::default()
                    });
                }
            } else if let Some(expr) = receiver {
                // Any other type: its own methods, from `impl i32 { … }`, then the methods of
                // the traits it has, `12.compare(&other)`.
                let t = a.typed.expr(expr);
                let viewed = match *a.typed.ty(t) {
                    Ty::Ref { to, .. } => to,
                    _ => t,
                };
                let owner = som_typeck::traits::primitive_of(a.typed.ty(viewed))
                    .and_then(|p| a.names.primitive(p));
                for (name, method) in owner.map(|o| a.names.methods_of(o)).unwrap_or_default() {
                    // Only those taking a `self`: `i32::parse` is called on the type.
                    let DefKind::Func(f) = a.names.def(method).kind else {
                        continue;
                    };
                    if a.ast.funcs[f]
                        .params
                        .first()
                        .is_none_or(|p| &*p.name.name != "self")
                    {
                        continue;
                    }
                    // A trait's, which comes with the trait methods below.
                    let from_trait =
                        (a.names.impl_of(f)).is_some_and(|i| a.names.impls[i].trait_.is_some());
                    if from_trait {
                        continue;
                    }
                    let snippet = call_snippet(a, f, name, 1, insert);
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(signature(a, f)),
                        documentation: markdown(doc_of(a, method)),
                        ..Default::default()
                    });
                }
                for (method, signature) in trait_methods_of(a, t) {
                    let name = a.ast.funcs[method].name.name.to_string();
                    let snippet = call_snippet(a, method, &name, 1, insert);
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name,
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(signature),
                        documentation: markdown(a.ast.funcs[method].doc.clone()),
                        ..Default::default()
                    });
                }
            }
        } else if let Some((stubs, start)) =
            impl_stubs(a, &cursor.file, &cursor.text, cursor.offset)
        {
            // In `impl Eq for Hello { … }`: what it still has to write, replacing the `fn e`
            // typed so far.
            let typed = cursor.offset - start;
            let range = Range {
                start: Position {
                    line: position.position.line,
                    character: position.position.character.saturating_sub(typed as u32),
                },
                end: position.position,
            };
            for stub in stubs {
                let keyword = if stub.is_type { "type" } else { "fn" };
                items.push(CompletionItem {
                    label: stub.signature.clone(),
                    kind: Some(match stub.is_type {
                        true => CompletionItemKind::TYPE_PARAMETER,
                        false => CompletionItemKind::METHOD,
                    }),
                    detail: Some("not written yet".into()),
                    filter_text: Some(format!("{keyword} {}", stub.name)),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range,
                        new_text: stub.snippet,
                    })),
                    preselect: Some(true),
                    ..Default::default()
                });
            }
            return Ok(Some(CompletionResponse::Array(items)));
        } else if let Some((path, written)) = struct_literal(&cursor.text, cursor.offset)
            && let Some(s) = find_struct(a, path, &cursor.file, cursor.offset)
        {
            // `Color { r: 1, ` lists the fields not written yet.
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
        } else if let Some(slot) = type_slot(&cursor.text, cursor.offset) {
            // `impl ` lists traits and types, `impl Add for ` types, `<T: ` traits: from the
            // modules in scope by name, others by theirs.
            let (here, in_scope) = modules_in_scope(a, &cursor.file, cursor.offset);
            let wants_traits = slot != TypeSlot::Type;
            let wants_types = slot != TypeSlot::Trait;
            for (m, module) in a.ast.modules.iter().enumerate() {
                let qualify = (!in_scope.contains(&m) && m != here && !module.name.is_empty())
                    .then_some(&*module.name);
                let name = |n: &str| match qualify {
                    Some(module) => format!("{module}::{n}"),
                    None => n.to_string(),
                };
                for item in &module.items {
                    match item {
                        Item::Trait(t) if wants_traits => {
                            items.push(trait_item(a, *t, name(&a.ast.traits[*t].name.name)));
                        }
                        Item::Struct(s) if wants_types => {
                            let st = &a.ast.structs[*s];
                            items.push(CompletionItem {
                                label: name(&st.name.name),
                                kind: Some(match st.is_enum() {
                                    true => CompletionItemKind::ENUM,
                                    false => CompletionItemKind::STRUCT,
                                }),
                                documentation: markdown(st.doc.clone()),
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                }
            }
            return Ok(Some(CompletionResponse::Array(items)));
        } else if let Some(module) = qualifier(&cursor.text, cursor.offset) {
            // `raylib::` lists what's in `raylib`.
            if let Some(m) = a.ast.modules.iter().position(|m| *m.name == *module) {
                items.extend(module_items(a, m, None, insert));
            } else if let Some(owner) = a.names.primitives_named(module) {
                // `i32::` lists its own functions and methods.
                for (name, method) in a.names.methods_of(owner) {
                    let DefKind::Func(f) = a.names.def(method).kind else {
                        continue;
                    };
                    let snippet = call_snippet(a, f, name, 0, insert);
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(signature(a, f)),
                        documentation: markdown(doc_of(a, method)),
                        ..Default::default()
                    });
                }
            } else if let Some(s) = find_struct(a, module, &cursor.file, cursor.offset) {
                // `Shape::` lists its variants, then its methods.
                for (i, variant) in a.ast.structs[s].variants().iter().enumerate() {
                    let snippet = variant_snippet(a, s, i, insert);
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: variant.name.name.to_string(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some(variant_text(a, s, i)),
                        documentation: markdown(variant.doc.clone()),
                        ..Default::default()
                    });
                }
                let owner = a.names.strukt(s);
                for (name, method) in a.names.methods_of(owner) {
                    let snippet = match a.names.def(method).kind {
                        DefKind::Func(f) => call_snippet(a, f, name, 0, insert),
                        _ => None,
                    };
                    items.push(CompletionItem {
                        insert_text_format: snippet.as_ref().map(|_| InsertTextFormat::SNIPPET),
                        insert_text: snippet,
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some(describe(a, method)),
                        documentation: markdown(doc_of(a, method)),
                        ..Default::default()
                    });
                }
            }
            return Ok(Some(CompletionResponse::Array(items)));
        } else {
            // The variables in scope here: parameters, and what's declared before the cursor.
            for def in locals_at(a, &cursor.file, cursor.offset) {
                let d = a.names.def(def);
                items.push(CompletionItem {
                    label: d.name.to_string(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some(describe(a, def)),
                    ..Default::default()
                });
            }
            // This file's module, and the modules it `use`s, by bare name; other modules by
            // theirs, so `rayl` finds `raylib::init_window`.
            let (here, in_scope) = modules_in_scope(a, &cursor.file, cursor.offset);
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
                    // What's in scope is offered by its bare name above.
                    if !in_scope.contains(&m) {
                        items.extend(module_items(a, m, Some(&module.name), insert));
                    }
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
                "break", "use", "out", "true", "false", "enum", "match", "trait", "type",
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

        // The inferred type after every unannotated `let`, and every `for` loop's name.
        let mut hints = Vec::new();
        for (id, stmt) in a.ast.stmts.iter() {
            let name = match stmt {
                Stmt::Let { name, ty: None, .. } | Stmt::For { name, .. } => name,
                _ => continue,
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
        hints.extend(return_type_hints(a, &file));
        // `let list: List = [1, 2]`: the type arguments left out, after the name.
        for (id, stmt) in a.ast.stmts.iter() {
            let Stmt::Let {
                ty: Some(annotation),
                ..
            } = stmt
            else {
                continue;
            };
            if a.ast.types.span(*annotation).source.identifier() != file {
                continue;
            }
            if let Some(t) = a.names.local(id).and_then(|def| a.typed.try_local(def)) {
                omitted_args_hints(a, *annotation, t, &mut hints);
            }
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
                Expr::Call { args, .. } => (a.typed.callee(id, &a.names), args, 0),
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
        // A value converted through `From`, which the source doesn't show: `List::from([1, 2])`.
        // After the parameter names, which go before it at the same place.
        for (id, _) in a.ast.exprs.iter() {
            let Some((_, &[target, source, ..])) = a.typed.conversion(id) else {
                continue;
            };
            let span = a.ast.exprs.span(id);
            if span.source.identifier() != file {
                continue;
            }
            let Ty::Struct(def, _) = a.typed.ty(target) else {
                continue;
            };
            let tooltip = format!(
                "converted to `{}` by its `From<{}>`",
                ty(a, target),
                ty(a, source)
            );
            let range = convert::span_to_range(span);
            for (position, label) in [
                (range.start, format!("{}::from(", a.names.def(*def).name)),
                (range.end, ")".to_string()),
            ] {
                hints.push(InlayHint {
                    position,
                    label: InlayHintLabel::String(label),
                    kind: None,
                    text_edits: None,
                    tooltip: Some(InlayHintTooltip::String(tooltip.clone())),
                    padding_left: None,
                    padding_right: None,
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
                    && let Some(sequence) = array_of(a, a.typed.expr(*receiver)) =>
            {
                let doc = array_method_doc(a, method, sequence);
                (doc.signature, doc.params, Some(doc.doc))
            }
            _ => {
                let (def, skip_self) = match expr {
                    Expr::Call { .. } => (a.typed.callee(call, &a.names), 0),
                    _ => (a.typed.method(call), 1),
                };
                let Some(DefKind::Func(func)) = def.map(|d| a.names.def(d).kind.clone()) else {
                    return Ok(None);
                };
                let subst = match expr {
                    Expr::MethodCall { receiver, .. } => {
                        method_subst(a, func, a.typed.expr(*receiver))
                    }
                    _ => Vec::new(),
                };
                let params = params_with(a, func, &subst)
                    .into_iter()
                    .skip(skip_self)
                    .collect();
                (
                    signature_with(a, func, &subst),
                    params,
                    a.ast.funcs[func].doc.clone(),
                )
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
        DefKind::Primitive(_) => d.name.to_string(),
        DefKind::Param { func, index } if a.ast.funcs[*func].params[*index].out => {
            let param = &a.ast.funcs[*func].params[*index];
            match a.typed.try_local(def) {
                Some(t) => param_text(a, param, t, &[]),
                None => format!("out {}", d.name),
            }
        }
        DefKind::Variant { strukt, index } => {
            let def = &a.ast.structs[*strukt];
            format!("{}::{}", def.name, variant_text(a, *strukt, *index))
        }
        DefKind::Param { .. } | DefKind::Local(_) | DefKind::Out(_) | DefKind::Binding(_) => {
            match a.typed.try_local(def) {
                Some(t) => format!("{}: {}", d.name, ty(a, t)),
                None => d.name.to_string(),
            }
        }
        DefKind::TypeParam { .. } | DefKind::StructParam { .. } | DefKind::ImplParam { .. } => {
            format!("type parameter {}", d.name)
        }
        DefKind::Trait(t) => {
            let t = &a.ast.traits[*t];
            let mut lines: Vec<String> = (t.types.iter())
                .map(|assoc| format!("    type {};", assoc.name))
                .collect();
            lines.extend(
                t.methods
                    .iter()
                    .map(|&m| format!("    {};", signature(a, m))),
            );
            format!("trait {} {{\n{}\n}}", t.name, lines.join("\n"))
        }
        DefKind::AssocType { .. } | DefKind::ImplType { .. } => format!("type {}", d.name),
        DefKind::SelfType { impl_ } => match a.names.impls[*impl_].owner {
            Some(owner) => format!("Self = {}", a.names.def(owner).name),
            None => "Self".to_string(),
        },
    }
}

/// What hovering at `offset` in `file` shows: the thing under the cursor as som, and its
/// documentation below it.
fn hover_markdown(a: &Analysis, file: &str, offset: usize) -> Option<String> {
    if let Some(block) = extern_at(a, file, offset) {
        return Some(extern_hover(a, block));
    }
    let node = hover_node(a, AstIndex::build(&a.ast).find_at(file, offset)?);

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
                match a.typed.callee(id, &a.names) {
                    Some(def) => (Some(describe(a, def)), doc_of(a, def)),
                    None => (None, None),
                }
            }
            Expr::MethodCall { receiver, .. } => match a.typed.method(id) {
                Some(def) => match a.names.def(def).kind {
                    DefKind::Func(f) => {
                        let subst = method_subst(a, f, a.typed.expr(*receiver));
                        (Some(signature_with(a, f, &subst)), doc_of(a, def))
                    }
                    _ => (Some(describe(a, def)), doc_of(a, def)),
                },
                None => match (
                    a.typed.array_method(id),
                    array_of(a, a.typed.expr(*receiver)),
                ) {
                    (Some(method), Some(sequence)) => {
                        let doc = array_method_doc(a, method, sequence);
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
            // `break` or `continue`, and the `x` in `break x;`, which names the loop like a
            // label: the loop it leaves or goes on with.
            Stmt::Break(_) | Stmt::Continue(_) => (
                loop_of(a, id).map(|l| {
                    let header = loop_header(a, l).get_text();
                    header.split_whitespace().collect::<Vec<_>>().join(" ")
                }),
                None,
            ),
            _ => (None, None),
        },
    };

    let text = text.filter(|t| !t.contains("{unknown}"))?;
    Some(match doc {
        Some(doc) => format!("```som\n{text}\n```\n\n{doc}"),
        None => format!("```som\n{text}\n```"),
    })
}

/// What hovering over `node` describes: usually `node` itself, but a literal bound of a range
/// is part of one thing, so `1` in `1..4` stands for the whole range. A bound that's a name or
/// a calculation, like `n` in `0..n`, stays itself: its type is worth seeing.
fn hover_node(a: &Analysis, node: NodeRef) -> NodeRef {
    let NodeRef::Expr(id) = node else {
        return node;
    };
    if !matches!(a.ast.exprs[id], Expr::Int(_) | Expr::Float(_)) {
        return node;
    }
    a.ast
        .exprs
        .iter()
        .find(|(_, e)| matches!(e, Expr::Range { start, end } if *start == Some(id) || *end == Some(id)))
        .map_or(node, |(range, _)| NodeRef::Expr(range))
}

/// The variables visible at `offset` in `file`: the parameters of the function it's in, and
/// the `let`, `for` and `out` variables declared before it in a block around it. A later
/// declaration of the same name hides an earlier one.
fn locals_at(a: &Analysis, file: &str, offset: usize) -> Vec<DefId> {
    let contains = |span: &Span| {
        span.source.identifier() == file
            && span.start_offset <= offset
            && offset <= span.start_offset + span.length
    };
    let Some((func, func_span)) = (a.ast.funcs.iter())
        .map(|(id, _)| (id, a.ast.funcs.span(id)))
        .filter(|(_, span)| contains(span))
        .min_by_key(|(_, span)| span.length)
    else {
        return Vec::new();
    };
    // What a declaration is visible in: the innermost block or loop around it, or for a `for`
    // loop's variable, the loop itself.
    let scopes: Vec<&Span> = (a.ast.exprs.iter())
        .filter(|(_, e)| matches!(e, Expr::Block { .. }))
        .map(|(id, _)| a.ast.exprs.span(id))
        .chain(
            (a.ast.stmts.iter())
                .filter(|(_, s)| {
                    matches!(
                        s,
                        Stmt::If { .. } | Stmt::While { .. } | Stmt::Loop { .. } | Stmt::For { .. }
                    )
                })
                .map(|(id, _)| a.ast.stmts.span(id)),
        )
        .collect();
    // The function's own body: while a line is still being typed, it can end before the
    // cursor, so what's declared in it is visible anywhere in the function.
    let body = a.ast.funcs[func].body.map(|b| a.ast.exprs.span(b));
    let scope_of = |declared: &Span| {
        let innermost = scopes
            .iter()
            .filter(|scope| {
                scope.source.identifier() == file
                    && scope.start_offset <= declared.start_offset
                    && declared.start_offset + declared.length <= scope.start_offset + scope.length
                    && scope.length > declared.length
            })
            .min_by_key(|scope| scope.length)
            .copied()?;
        let is_body = body.is_some_and(|b| {
            b.start_offset == innermost.start_offset && b.length == innermost.length
        });
        Some(if is_body { func_span } else { innermost })
    };

    let mut found: Vec<(usize, DefId)> = a
        .names
        .params(func)
        .iter()
        .map(|&def| (a.names.def(def).span.start_offset, def))
        .collect();
    for (def, d) in a.names.defs.iter_with_ids() {
        let scope = match d.kind {
            // A `for` loop's variable is visible in the loop.
            DefKind::Local(stmt) if matches!(a.ast.stmts[stmt], Stmt::For { .. }) => {
                Some(a.ast.stmts.span(stmt))
            }
            DefKind::Local(stmt) => scope_of(a.ast.stmts.span(stmt)),
            DefKind::Out(expr) => scope_of(a.ast.exprs.span(expr)),
            _ => continue,
        };
        let declared_before = d.span.source.identifier() == file && d.span.start_offset < offset;
        let in_function = func_span.start_offset <= d.span.start_offset
            && d.span.start_offset <= func_span.start_offset + func_span.length;
        if declared_before && in_function && scope.is_some_and(contains) {
            found.push((d.span.start_offset, def));
        }
    }
    // The latest declaration of each name wins.
    found.sort_by_key(|&(at, _)| std::cmp::Reverse(at));
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|&(_, def)| seen.insert(a.names.def(def).name.clone()))
        .map(|(_, def)| def)
        .collect()
}

/// Where `break`, `continue` or `break x` is written, without the `;`: the one thing hover
/// and go to definition point at.
fn jump_span(a: &Analysis, stmt: Id<Stmt>) -> Option<Span> {
    let (Stmt::Break(named) | Stmt::Continue(named)) = &a.ast.stmts[stmt] else {
        return None;
    };
    let whole = a.ast.stmts.span(stmt);
    let end = match named {
        Some(name) => name.span.start_offset + name.span.length,
        None => {
            let keyword = match a.ast.stmts[stmt] {
                Stmt::Break(_) => "break",
                _ => "continue",
            };
            whole.start_offset + keyword.len()
        }
    };
    Some(Span::from_range(
        whole.start_offset..end,
        whole.source.clone(),
    ))
}

/// The loop `stmt` leaves or goes on with, if it's `break` or `continue`: the `for` loop it
/// names, or else the innermost loop around it.
fn loop_of(a: &Analysis, stmt: Id<Stmt>) -> Option<Id<Stmt>> {
    let (Stmt::Break(named) | Stmt::Continue(named)) = &a.ast.stmts[stmt] else {
        return None;
    };
    if named.is_some() {
        return match a.names.def(a.names.loop_target(stmt)?).kind {
            DefKind::Local(for_stmt) => Some(for_stmt),
            _ => None,
        };
    }
    // Loops nest inside one another, so the innermost is the shortest around it.
    let at = a.ast.stmts.span(stmt);
    let around = |span: &Span| {
        span.source.identifier() == at.source.identifier()
            && span.start_offset <= at.start_offset
            && at.start_offset + at.length <= span.start_offset + span.length
    };
    a.ast
        .stmts
        .iter()
        .filter(|(_, s)| matches!(s, Stmt::Loop { .. } | Stmt::While { .. } | Stmt::For { .. }))
        .filter(|(id, _)| around(a.ast.stmts.span(*id)))
        .min_by_key(|(id, _)| a.ast.stmts.span(*id).length)
        .map(|(id, _)| id)
}

/// Where a loop's header is written, without its body: `for x in 1..4`, `while i < n`, `loop`.
fn loop_header(a: &Analysis, loop_stmt: Id<Stmt>) -> Span {
    let whole = a.ast.stmts.span(loop_stmt);
    let end = match &a.ast.stmts[loop_stmt] {
        Stmt::For { iterable: last, .. }
        | Stmt::While {
            condition: last, ..
        } => {
            let last = a.ast.exprs.span(*last);
            last.start_offset + last.length
        }
        _ => whole.start_offset + "loop".len(),
    };
    Span::from_range(whole.start_offset..end, whole.source.clone())
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
    readable_fields(a, s).get(index?)?.0.doc.clone()
}

/// What `value.field` can read on a value of struct `s`: its fields; or for an enum, the
/// fields every variant has. Each with its declared type.
fn readable_fields(a: &Analysis, s: Id<Struct>) -> Vec<(&som_ast::Field, som_typeck::TypeId)> {
    let st = &a.ast.structs[s];
    let types = a.typed.fields(s);
    if !st.is_enum() {
        return st.fields.iter().zip(types.iter().copied()).collect();
    }
    let payload: Vec<&som_ast::Field> = st.payload().collect();
    (a.typed.common_fields(s).iter())
        .map(|c| (payload[c.payload[0]], types[c.payload[0]]))
        .collect()
}

/// A typed pointer method's parameters (after the pointer), its result, and what it does;
/// `T` is what the pointer points to.
fn pointer_method_doc(
    method: PointerMethod,
) -> (
    &'static [(&'static str, &'static str)],
    &'static str,
    &'static str,
) {
    match method {
        PointerMethod::Resize => (
            &[("count", "usize")],
            "",
            "Makes it room for `count` values, keeping the ones there (as many as fit). A null \
             one starts empty; copies of the old pointer aren't valid any more.",
        ),
        PointerMethod::Free => (&[], "", "Gives the memory back."),
        PointerMethod::Read => (
            &[("index", "usize")],
            "T",
            "The value at `index`, copied or moved out. Nothing checks it's there.",
        ),
        PointerMethod::Write => (
            &[("index", "usize"), ("value", "T")],
            "",
            "Stores `value` at `index`, without reading or freeing what was there.",
        ),
        PointerMethod::At => (
            &[("index", "usize")],
            "&T",
            "A reference to the value at `index`.",
        ),
        PointerMethod::AtMut => (
            &[("index", "usize")],
            "&mut T",
            "A reference to the value at `index`, to change it.",
        ),
        PointerMethod::View => (
            &[("len", "usize")],
            "[T]",
            "The `len` values starting here, as a slice.",
        ),
        PointerMethod::ViewMut => (
            &[("len", "usize")],
            "[mut T]",
            "The `len` values starting here, as a slice that can change them.",
        ),
        PointerMethod::IsNull => (&[], "bool", "Whether it points to nothing."),
    }
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

/// The parameters of `f`, with type parameters shown as what `subst` says they are.
fn params_with(a: &Analysis, f: Id<Func>, subst: &[(DefId, som_typeck::TypeId)]) -> Vec<String> {
    a.ast.funcs[f]
        .params
        .iter()
        .zip(&a.typed.func(f).params)
        .map(|(p, &t)| param_text(a, p, t, subst))
        .collect()
}

/// `width: i32`, or `out remainder: i32` for a parameter whose type is `&mut i32`
/// underneath.
fn param_text(
    a: &Analysis,
    p: &som_ast::Param,
    t: som_typeck::TypeId,
    subst: &[(DefId, som_typeck::TypeId)],
) -> String {
    let shown = |t| a.typed.table.display_with(t, &a.names, subst);
    match (p.out, a.typed.ty(t)) {
        (true, Ty::Ref { to, .. }) => format!("out {}: {}", p.name, shown(*to)),
        _ => format!("{}: {}", p.name, shown(t)),
    }
}

/// What method `f`'s type parameters are when it's called on `receiver`: `T` is `i32` for a
/// `Vector<T>` method called on a `Vector<i32>`, worked out from its `self` parameter.
fn method_subst(
    a: &Analysis,
    f: Id<Func>,
    receiver: som_typeck::TypeId,
) -> Vec<(DefId, som_typeck::TypeId)> {
    let unref = |t: som_typeck::TypeId| match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    let Some(&self_ty) = a.typed.func(f).params.first() else {
        return Vec::new();
    };
    match (a.typed.ty(unref(self_ty)), a.typed.ty(unref(receiver))) {
        (Ty::Struct(_, written), Ty::Struct(_, actual)) => written
            .iter()
            .zip(actual)
            .filter_map(|(&w, &actual)| match a.typed.ty(w) {
                Ty::Param(def) => Some((*def, actual)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `fn name<T>(a: i32, b: T) -> i32`, or `extern fn ...` for externs.
fn signature(a: &Analysis, f: Id<Func>) -> String {
    signature_with(a, f, &[])
}

/// `f`'s signature with type parameters shown as what `subst` says they are; those it settles
/// are left out of the `<…>`.
fn signature_with(a: &Analysis, f: Id<Func>, subst: &[(DefId, som_typeck::TypeId)]) -> String {
    let func = &a.ast.funcs[f];
    let sig = a.typed.func(f);
    let open: Vec<String> = (func.generics.iter())
        .zip(a.names.type_params(f))
        .filter(|(_, def)| !subst.iter().any(|(d, _)| d == *def))
        .map(|(p, _)| match p.ty {
            Some(ty) => format!("{}: {}", p.name.name, a.ast.type_text(ty)),
            None => p.name.name.to_string(),
        })
        .collect();
    let generics = if open.is_empty() {
        String::new()
    } else {
        format!("<{}>", open.join(", "))
    };
    let ret = if matches!(a.typed.ty(sig.ret), Ty::Unit) {
        String::new()
    } else {
        format!(
            " -> {}",
            a.typed.table.display_with(sig.ret, &a.names, subst)
        )
    };
    let prefix = if func.body.is_none() { "extern " } else { "" };
    let mut params = params_with(a, f, subst);
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
    if def.is_enum() {
        let variants: Vec<String> = (0..def.variants().len())
            .map(|i| format!("    {},", variant_text(a, s, i)))
            .collect();
        return format!("enum {} {{\n{}\n}}", def.name, variants.join("\n"));
    }
    let fields: Vec<String> = def
        .fields
        .iter()
        .zip(a.typed.fields(s))
        .map(|(f, &t)| format!("    {}: {},", f.name, ty(a, t)))
        .collect();
    format!("struct {} {{\n{}\n}}", def.name, fields.join("\n"))
}

/// Variant `index` of enum `s` as declared: `Circle(f32)`, `Rect { w: f32, h: f32 }`.
fn variant_text(a: &Analysis, s: Id<Struct>, index: usize) -> String {
    let def = &a.ast.structs[s];
    let variant = &def.variants()[index];
    let start = def.payload_start(index);
    let types = &a.typed.fields(s)[start..start + variant.fields.len()];
    match variant.kind {
        som_ast::VariantKind::Unit => variant.name.to_string(),
        som_ast::VariantKind::Tuple => {
            let types: Vec<String> = types.iter().map(|&t| ty(a, t)).collect();
            format!("{}({})", variant.name, types.join(", "))
        }
        som_ast::VariantKind::Named => {
            let fields: Vec<String> = (variant.fields.iter().zip(types))
                .map(|(f, &t)| format!("{}: {}", f.name, ty(a, t)))
                .collect();
            format!("{} {{ {} }}", variant.name, fields.join(", "))
        }
    }
}

/// The struct a value of type `t` is, looking through one reference.
/// What the editor needs to describe an array's, slice's or text's methods.
#[derive(Clone, Copy)]
struct Sequence {
    element: som_typeck::TypeId,
    /// How many elements, if that's fixed.
    len: Option<u64>,
    is_text: bool,
}

/// The array, slice or text `t` is, looking through one reference.
fn array_of(a: &Analysis, t: som_typeck::TypeId) -> Option<Sequence> {
    let t = match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    let (element, len, is_text) = match a.typed.ty(t) {
        Ty::Array { element, len } => {
            let len = match a.typed.ty(*len) {
                Ty::Value(som_typeck::Value::Int(n)) => Some(*n as u64),
                // `N` in a generic function: only known in each copy of it.
                _ => None,
            };
            (*element, len, false)
        }
        Ty::Slice(element) => (*element, None, false),
        // Text is its bytes.
        Ty::Str => (a.typed.table.u8, None, true),
        _ => return None,
    };
    Some(Sequence {
        element,
        len,
        is_text,
    })
}

/// One of the methods every array has, as the editor shows it.
struct ArrayMethodDoc {
    name: &'static str,
    signature: String,
    params: Vec<String>,
    doc: String,
}

/// How `method` reads for `sequence`.
fn array_method_doc(a: &Analysis, method: ArrayMethod, sequence: Sequence) -> ArrayMethodDoc {
    let Sequence {
        element,
        len,
        is_text,
    } = sequence;
    let t = ty(a, element);
    let range = "range: Range<any integer>".to_string();
    let part = "or `none` if the range isn't within them. An end left out, as in `1..`, is \
                their own.";
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
        ArrayMethod::Slice => (
            "slice",
            vec![range],
            if is_text {
                "str?".to_string()
            } else {
                format!("[{t}]?")
            },
            format!("A view of the elements in `range`, {part}"),
        ),
        ArrayMethod::SliceMut => (
            "slice_mut",
            vec![range],
            format!("[mut {t}]?"),
            format!("A view of the elements in `range` that can change them, {part}"),
        ),
        ArrayMethod::Bytes => (
            "bytes",
            vec![],
            "[u8]".to_string(),
            "The text's bytes, as a slice: the same view, so it costs nothing.".to_string(),
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

/// What struct `s`'s type parameters stand for in `t`, a use of it (or a reference to one):
/// `T` is `i32` in a `Vector<i32>`.
fn struct_subst(
    a: &Analysis,
    s: Id<Struct>,
    t: som_typeck::TypeId,
) -> Vec<(DefId, som_typeck::TypeId)> {
    let t = match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    match a.typed.ty(t) {
        Ty::Struct(_, args) => (a.names.struct_params(s).iter().copied())
            .zip(args.iter().copied())
            .collect(),
        _ => Vec::new(),
    }
}

fn struct_of(a: &Analysis, t: som_typeck::TypeId) -> Option<DefId> {
    let t = match a.typed.ty(t) {
        Ty::Ref { to, .. } => *to,
        _ => t,
    };
    match a.typed.ty(t) {
        Ty::Struct(def, _) => Some(*def),
        _ => None,
    }
}

/// Where the field read by `object.field` is declared.
fn field_definition(a: &Analysis, object: Id<Expr>, index: Option<usize>) -> Option<Span> {
    let owner = struct_of(a, a.typed.expr(object))?;
    let DefKind::Struct(s) = a.names.def(owner).kind else {
        return None;
    };
    Some(readable_fields(a, s).get(index?)?.0.name.span.clone())
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
fn modules_in_scope(a: &Analysis, file: &str, offset: usize) -> (usize, Vec<usize>) {
    let here = a
        .ast
        .modules
        .iter()
        .position(|m| m.files.iter().any(|f| f.identifier() == file))
        .unwrap_or(0);
    let mut in_scope = vec![here];
    for item in &a.ast.modules[here].items {
        // `use som::io;` too: modules are named by their whole path.
        if let Item::Use(u) = item
            && let name = u.path.to_string()
            && let Some(m) = a.ast.modules.iter().position(|m| *m.name == *name)
        {
            in_scope.push(m);
        }
    }
    // `use som::data;` inside a block, for the rest of it: if the cursor is there.
    for (id, stmt) in a.ast.stmts.iter() {
        let Stmt::Use(u) = stmt else { continue };
        let at = a.ast.stmts.span(id);
        if at.source.identifier() != file || at.start_offset + at.length > offset {
            continue;
        }
        // The innermost block around it.
        let block = (a.ast.exprs.iter())
            .filter(|(_, e)| matches!(e, Expr::Block { .. }))
            .map(|(e, _)| a.ast.exprs.span(e))
            .filter(|b| {
                b.source.identifier() == file
                    && b.start_offset <= at.start_offset
                    && at.start_offset + at.length <= b.start_offset + b.length
            })
            .min_by_key(|b| b.length);
        let inside = block.is_some_and(|b| offset < b.start_offset + b.length);
        let module = a
            .ast
            .modules
            .iter()
            .position(|m| *m.name == *u.path.to_string());
        if inside
            && let Some(m) = module
            && !in_scope.contains(&m)
        {
            in_scope.push(m);
        }
    }
    // The standard library's root module is seen from everywhere.
    if let Some(prelude) = a.ast.modules.iter().position(|m| &*m.name == "som")
        && !in_scope.contains(&prelude)
    {
        in_scope.push(prelude);
    }
    (here, in_scope)
}

/// The struct a literal names: `raylib::Color` in that module, `Color` in the modules
/// in scope.
fn find_struct(a: &Analysis, path: &str, file: &str, offset: usize) -> Option<Id<Struct>> {
    let modules = match path.rsplit_once("::") {
        Some((module, _)) => a
            .ast
            .modules
            .iter()
            .position(|m| *m.name == *module)
            .into_iter()
            .collect(),
        None => modules_in_scope(a, file, offset).1,
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
    // `fn f() -> i32 {` is a body, not a literal.
    if matches!(keyword, "struct" | "impl" | "fn") || before[..start].trim_end().ends_with("->") {
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
/// In `use som::da`: the path before the part being typed, `som::`; `""` right after `use`.
fn use_path(text: &str, offset: usize) -> Option<&str> {
    let before = text.get(..offset)?;
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let path = line.trim_start().strip_prefix("use ")?.trim_start();
    if !path
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == ':')
    {
        return None;
    }
    let typed = path.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
    Some(typed)
}

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
            Item::Trait(id) => items.push(trait_item(a, *id, name(&a.ast.traits[*id].name.name))),
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
        | Expr::Neg(_)
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

/// `<i32>` after each generic struct in annotation `written` whose type arguments are left
/// out, from `t`, the type it turned out to be.
fn omitted_args_hints(
    a: &Analysis,
    written: Id<som_ast::TypeExpr>,
    t: som_typeck::TypeId,
    hints: &mut Vec<InlayHint>,
) {
    use som_ast::TypeExpr;
    match (&a.ast.types[written], a.typed.ty(t)) {
        (TypeExpr::Named { args, .. }, Ty::Struct(_, actual)) if args.is_empty() => {
            if actual.is_empty() {
                return;
            }
            let shown: Vec<String> = actual.iter().map(|&arg| ty(a, arg)).collect();
            hints.push(InlayHint {
                position: convert::span_to_range(a.ast.types.span(written)).end,
                label: InlayHintLabel::String(format!("<{}>", shown.join(", "))),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: None,
                padding_right: None,
                data: None,
            });
        }
        (TypeExpr::Named { args, .. }, Ty::Struct(_, actual)) => {
            for (&arg, &actual) in args.iter().zip(actual) {
                omitted_args_hints(a, arg, actual, hints);
            }
        }
        (TypeExpr::Optional(inner), Ty::Optional(actual))
        | (TypeExpr::Reference { to: inner, .. }, Ty::Ref { to: actual, .. })
        | (
            TypeExpr::Array { element: inner, .. },
            Ty::Array {
                element: actual, ..
            },
        ) => {
            omitted_args_hints(a, *inner, *actual, hints);
        }
        _ => {}
    }
}

/// `: i32` after the name a variable is declared with.
/// The inferred return type of each function, method or property in `file` that doesn't
/// write one, right after its parameters: `fn double(x: i32) -> i32`. Double-clicking writes
/// it in.
fn return_type_hints(a: &Analysis, file: &str) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    for (id, func) in a.ast.funcs.iter() {
        let (None, Some(body)) = (func.returns, func.body) else {
            continue;
        };
        let at = a.ast.exprs.span(body);
        if at.source.identifier() != file {
            continue;
        }
        let ret = a.typed.func(id).ret;
        if matches!(a.typed.ty(ret), Ty::Unit | Ty::Error) {
            continue;
        }
        // Right after the parameters' `)`: with no return type written, only space comes
        // between it and the body.
        let before = at.source.content()[..at.start_offset].trim_end();
        if !before.ends_with(')') {
            continue;
        }
        let close = Span::from_range(before.len()..before.len(), at.source.clone());
        let position = convert::span_to_range(&close).start;
        let text = format!(" -> {}", ty(a, ret));
        hints.push(InlayHint {
            position,
            label: InlayHintLabel::String(text.clone()),
            kind: Some(InlayHintKind::TYPE),
            text_edits: Some(vec![TextEdit {
                range: Range::new(position, position),
                new_text: text.clone(),
            }]),
            tooltip: None,
            padding_left: None,
            padding_right: None,
            data: None,
        });
    }
    hints
}

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
    let snippet = call_snippet(a, f, &label, 0, insert);
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

/// `label(${1:a}, ${2:b})$0`: a call of `f` with a tab stop on each argument after the first
/// `skip` (a method's `self`, for `v.len()`). None if a call already follows.
fn call_snippet(
    a: &Analysis,
    f: Id<Func>,
    label: &str,
    skip: usize,
    insert: Insert,
) -> Option<String> {
    if insert.call_follows {
        return None;
    }
    let args: Vec<String> = (a.ast.funcs[f].params.iter().skip(skip).enumerate())
        .map(|(i, p)| {
            let out = if p.out { "out " } else { "" };
            format!("{out}${{{}:{}}}", i + 1, p.name.name)
        })
        .collect();
    Some(format!("{label}({})$0", args.join(", ")))
}

/// Variant `index` of enum `s` as a value to fill in: `Circle(${1:f32})`, `Rect { w: $1, h: $2 }`.
fn variant_snippet(a: &Analysis, s: Id<Struct>, index: usize, insert: Insert) -> Option<String> {
    let def = &a.ast.structs[s];
    let variant = &def.variants()[index];
    let start = def.payload_start(index);
    let types = &a.typed.fields(s)[start..start + variant.fields.len()];
    let name = &variant.name.name;
    match variant.kind {
        som_ast::VariantKind::Unit => None,
        som_ast::VariantKind::Tuple if insert.call_follows => None,
        som_ast::VariantKind::Tuple => {
            let args: Vec<String> = (types.iter().enumerate())
                .map(|(i, &t)| format!("${{{}:{}}}", i + 1, ty(a, t)))
                .collect();
            Some(format!("{name}({})$0", args.join(", ")))
        }
        som_ast::VariantKind::Named => {
            let fields: Vec<String> = (variant.fields.iter().enumerate())
                .map(|(i, f)| format!("{}: ${}", f.name, i + 1))
                .collect();
            Some(format!("{name} {{ {} }}$0", fields.join(", ")))
        }
    }
}

/// `Color { r: $1, g: $2, b: $3, a: $4 }` where a value goes; just the name in a type.
/// A struct with many fields gets empty braces, and field completion inside them.
fn trait_item(a: &Analysis, t: Id<som_ast::Trait>, label: String) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::INTERFACE),
        detail: Some(format!("trait {}", a.ast.traits[t].name.name)),
        documentation: markdown(a.ast.traits[t].doc.clone()),
        ..Default::default()
    }
}

/// Inside a trait's impl block, where an item starts, with maybe `fn` or `type` and part of a
/// name typed: what the block still has to write, and where what's typed of it starts.
fn impl_stubs(
    a: &Analysis,
    file: &str,
    text: &str,
    offset: usize,
) -> Option<(Vec<som_typeck::stubs::Stub>, usize)> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let word_start = text[..offset].trim_end_matches(word).len();
    let line_start = text[..word_start].rfind('\n').map_or(0, |i| i + 1);
    let indent =
        text[line_start..word_start].len() - text[line_start..word_start].trim_start().len();
    let start = match text[line_start..word_start].trim() {
        "" => word_start,
        "fn" | "type" => line_start + indent,
        _ => return None,
    };
    let blocks = (a.ast.modules.iter())
        .flat_map(|m| &m.items)
        .filter_map(|item| match item {
            Item::Impl(block) => Some(block),
            _ => None,
        });
    for (index, block) in blocks.enumerate() {
        let header = block.target.span();
        if block.trait_.is_none()
            || block.close.source.identifier() != file
            || !(header.start_offset + header.length < offset && offset <= block.close.start_offset)
        {
            continue;
        }
        let in_method = block.methods.iter().any(|&m| {
            let span = a.ast.funcs.span(m);
            span.start_offset < start && offset <= span.start_offset + span.length
        });
        if in_method {
            return None;
        }
        let stubs = som_typeck::stubs::missing(&a.ast, &a.names, index, block);
        return (!stubs.is_empty()).then_some((stubs, start));
    }
    None
}

/// The methods of the traits values of type `t` have, each a trait's method with its
/// signature for `t`: the language's traits it has built in (`compare` of a number), then
/// the traits implemented for it.
fn trait_methods_of(a: &Analysis, t: som_typeck::TypeId) -> Vec<(Id<Func>, String)> {
    // Each trait with what its parameters are for `t`: `Double<i32>` gives `T` as `i32`.
    let mut traits: Vec<(Id<som_ast::Trait>, Vec<String>)> = Vec::new();
    let mut add = |def, args: Vec<String>| {
        if let DefKind::Trait(tr) = a.names.def(def).kind
            && !traits.iter().any(|(t, a)| *t == tr && *a == args)
        {
            traits.push((tr, args));
        }
    };
    for lang in LangTrait::ALL {
        if som_typeck::traits::builtin_operator(a.typed.ty(t), lang)
            && let Some(def) = a.names.lang(lang)
        {
            add(def, Vec::new());
        }
    }
    for (i, info) in a.typed.impls.iter().enumerate() {
        let Some(trait_) = info.trait_ else { continue };
        let this = std::slice::from_ref(&a.typed.impls[i]);
        let Some((_, args)) =
            som_typeck::traits::find_impl(this, &a.typed.table, trait_, t, &[], &|x| x)
        else {
            continue;
        };
        // The trait's arguments as this implementation gives them, for `t`.
        let subst: Vec<_> = info.params.iter().copied().zip(args).collect();
        let shown = (info.trait_args.iter())
            .map(|&arg| a.typed.table.display_with(arg, &a.names, &subst))
            .collect();
        add(trait_, shown);
    }
    let shown = ty(a, t);
    let mut methods = Vec::new();
    for (tr, args) in traits {
        let stubs = som_typeck::stubs::stubs(&a.ast, tr, &shown, &args, &|_, _| false);
        for &method in &a.ast.traits[tr].methods {
            let name = &a.ast.funcs[method].name.name;
            let signature = (stubs.iter())
                .find(|s| !s.is_type && *s.name == **name)
                .map_or_else(|| format!("fn {name}"), |s| s.signature.clone());
            methods.push((method, signature));
        }
    }
    methods
}

/// What can be written at the cursor in an `impl` header or a generic's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TypeSlot {
    /// `impl ` or `impl<T> `: a trait, or the type the methods are for.
    TraitOrType,
    /// `impl Add for `: the type.
    Type,
    /// `fn f<T: ` or `T: Eq + `: a trait.
    Trait,
}

/// Whether the cursor is in an `impl` header or a generic's bounds, and what goes there.
fn type_slot(text: &str, offset: usize) -> Option<TypeSlot> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..offset].trim_end_matches(word);
    let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("impl")
        && !rest.starts_with(word)
    {
        // Past `<T, N: usize>`.
        let rest = match rest.trim_start().strip_prefix('<') {
            Some(generics) => match generics.find('>') {
                Some(end) => &generics[end + 1..],
                None => return bound_slot(line),
            },
            None => rest,
        };
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(TypeSlot::TraitOrType);
        }
        if rest.ends_with(" for") || rest == "for" {
            return Some(TypeSlot::Type);
        }
        return None;
    }
    bound_slot(line)
}

/// In `fn f<T: ` or `struct S<K: Hash + `: after a `:` or `+` inside the `<…>` of a
/// declaration's generics.
fn bound_slot(line: &str) -> Option<TypeSlot> {
    let trimmed = line.trim_end();
    if !(trimmed.ends_with(':') && !trimmed.ends_with("::") || trimmed.ends_with('+')) {
        return None;
    }
    let mut depth = 0;
    for (i, c) in trimmed.char_indices().rev() {
        match c {
            '>' => depth += 1,
            '<' if depth > 0 => depth -= 1,
            '<' => {
                let head = trimmed[..i].trim_end();
                let keyword = head
                    .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_')
                    .trim_end();
                let declares = ["fn", "struct", "enum", "trait"]
                    .iter()
                    .any(|k| keyword.ends_with(k))
                    || head.ends_with("impl");
                return declares.then_some(TypeSlot::Trait);
            }
            '(' | ')' | '{' | '}' | ';' | '=' => return None,
            _ => {}
        }
    }
    None
}

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
            | TokenKind::Prop
            | TokenKind::Extern
            | TokenKind::Struct
            | TokenKind::Enum
            | TokenKind::Trait
            | TokenKind::Type
            | TokenKind::Match
            | TokenKind::Impl
            | TokenKind::Let
            | TokenKind::If
            | TokenKind::Else
            | TokenKind::Mut
            | TokenKind::Out
            | TokenKind::Use
            | TokenKind::And
            | TokenKind::As
            | TokenKind::Or
            | TokenKind::Loop
            | TokenKind::While
            | TokenKind::Break
            | TokenKind::Continue
            | TokenKind::Return
            | TokenKind::For
            | TokenKind::In => 0,
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
            | TokenKind::Question
            | TokenKind::DotDot => 6,
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
    fn inferred_return_types_are_hinted_after_the_parameters() {
        let dir = std::env::temp_dir().join(format!("som_lsp_return_hints_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let main = dir.join("main.som");
        let text = "struct Rect { w: i32 }\n\
                    impl Rect {\n    prop area(self: &Rect) { self.w * 2 }\n}\n\
                    fn double(x: i32) { x * 2 }\n\
                    fn written(x: i32) -> i32 { x }\n\
                    fn nothing() { }\n\
                    fn main() -> i32 { double(Rect { w: 1 }.area) }\n";
        std::fs::write(&main, text).unwrap();
        let analysis = som_driver::analyze(&dir, &som_driver::Options::default());
        let file = main.to_string_lossy();
        let labels: Vec<(u32, u32, String)> = return_type_hints(&analysis, &file)
            .into_iter()
            .map(|h| match h.label {
                InlayHintLabel::String(label) => (h.position.line, h.position.character, label),
                InlayHintLabel::LabelParts(_) => unreachable!(),
            })
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        // Right after the `)`. Not `written`, which says it, nor `nothing`, which gives
        // nothing back.
        assert_eq!(
            labels,
            [
                (2, 26, " -> i32".to_string()),
                (4, 17, " -> i32".to_string())
            ]
        );
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
    fn impl_headers_and_bounds_take_traits_and_types() {
        for (text, slot) in [
            ("impl ", Some(TypeSlot::TraitOrType)),
            ("impl Ad", Some(TypeSlot::TraitOrType)),
            ("impl<T> Ite", Some(TypeSlot::TraitOrType)),
            ("impl Add for ", Some(TypeSlot::Type)),
            ("impl<T> From<[T; 3]> for Li", Some(TypeSlot::Type)),
            ("fn sum<T: ", Some(TypeSlot::Trait)),
            ("fn sum<T: Add + Co", Some(TypeSlot::Trait)),
            ("impl<T: Eq", Some(TypeSlot::Trait)),
            ("struct Map<K: Ha", Some(TypeSlot::Trait)),
            ("let x: ", None),
            ("    impl_thing", None),
            ("impl Add for V { fn ", None),
            ("let y = a < b + ", None),
        ] {
            assert_eq!(type_slot(text, text.len()), slot, "{text}");
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
    fn use_paths_are_what_comes_before_the_word() {
        fn at_end(text: &str) -> Option<&str> {
            use_path(text, text.len())
        }
        assert_eq!(at_end("use "), Some(""));
        assert_eq!(at_end("use som::"), Some("som::"));
        assert_eq!(at_end("use som::da"), Some("som::"));
        assert_eq!(at_end("    use ray"), Some(""));
        assert_eq!(at_end("let use_it = "), None);
        assert_eq!(at_end("use som; let x = "), None);
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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use som_base::Source;
use som_driver::Analysis;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::request::{GotoDeclarationParams, GotoDeclarationResponse};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::convert;
use crate::handlers::completion::{self, Request};
use crate::handlers::semantic_tokens::{TOKEN_MODIFIERS, TOKEN_TYPES, semantic_tokens};
use crate::handlers::tests::{self, TestStatus};
use crate::handlers::{Cursor, definition, hover, inlay, lens, rename, signature_help, symbols};
use crate::lookup::receiver_before;
use crate::text_scan::dot_before_word;

/// How long typing has to pause before the program is analyzed again.
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(200);

/// How long a program's tests may run, all together, before they're stopped.
const TEST_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The analysis of one program.
struct Project {
    analysis: Analysis,
    /// Libraries and modules the program could import.
    importable: som_driver::Importable,
    /// Files diagnostics were published for, so they can be cleared next time.
    files: HashSet<String>,
    /// The analysis's diagnostics, by file: published with the tests' failures, again when
    /// the tests have run.
    diagnostics: HashMap<String, Vec<Diagnostic>>,
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
    refresh: AtomicBool,
    /// Each program's tests, as they last ran, by its root.
    tests: RwLock<HashMap<PathBuf, TestStatus>>,
    /// Held while tests run: one program's at a time.
    testing: tokio::sync::Mutex<()>,
    /// Counts the test runs asked for: one that's no longer the latest when its turn comes
    /// is skipped, since a newer one will run what's saved now.
    test_runs: AtomicU64,
}

impl State {
    /// The open files' text, to analyze in place of what's on disk.
    async fn overlay(&self) -> HashMap<PathBuf, String> {
        (self.documents.read().await.iter())
            .map(|(file, (_, text))| (PathBuf::from(file), text.clone()))
            .collect()
    }
}

/// The standard library's root, if `path` is in it: its files are one program.
fn std_root_of(path: &Path) -> Option<&'static Path> {
    som_driver::stdlib::root().filter(|_| som_driver::stdlib::contains(path))
}

/// The root of the program `path` belongs to: the standard library's, or else the nearest
/// folder with a `main.som`.
fn root_of(path: &Path) -> PathBuf {
    std_root_of(path).map_or_else(|| som_driver::program_root(path), Path::to_path_buf)
}

fn file_of(uri: &Url) -> Option<String> {
    Some(uri.to_file_path().ok()?.to_string_lossy().into_owned())
}

/// The project a file belongs to: the one rooted at its directory, or any project that
/// loaded it.
fn project_of<'a>(projects: &'a HashMap<PathBuf, Project>, file: &str) -> Option<&'a Project> {
    if let Some(project) = std_root_of(Path::new(file)).and_then(|root| projects.get(root)) {
        return Some(project);
    }
    let dir = Path::new(file).parent()?;
    projects.get(dir).or_else(|| {
        projects
            .values()
            .find(|p| p.analysis.ast.modules.iter().any(|m| m.path == dir))
    })
}

impl SomLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::default(),
        }
    }

    /// The text of a file: as it is in the editor if it's open, or else on disk.
    async fn text_of(&self, file: &str) -> Option<String> {
        match self.state.documents.read().await.get(file) {
            Some((_, text)) => Some(text.clone()),
            None => std::fs::read_to_string(file).ok(),
        }
    }

    async fn cursor(&self, at: &TextDocumentPositionParams) -> Option<Cursor> {
        let file = file_of(&at.text_document.uri)?;
        let text = self.text_of(&file).await?;
        let offset = convert::position_to_offset(&text, &at.position)?;
        Some(Cursor { file, text, offset })
    }

    /// `answer` from the analysis of the program the cursor at `at` is in.
    async fn at_cursor<R>(
        &self,
        at: &TextDocumentPositionParams,
        answer: impl FnOnce(&Analysis, &Cursor) -> Option<R>,
    ) -> Option<R> {
        let cursor = self.cursor(at).await?;
        let projects = self.state.projects.read().await;
        answer(&project_of(&projects, &cursor.file)?.analysis, &cursor)
    }

    /// `answer` from the analysis of the program `uri` is in.
    async fn in_file<R>(
        &self,
        uri: &Url,
        answer: impl FnOnce(&Analysis, &str) -> Option<R>,
    ) -> Option<R> {
        let file = file_of(uri)?;
        let projects = self.state.projects.read().await;
        answer(&project_of(&projects, &file)?.analysis, &file)
    }

    async fn analyze(&self, uri: &Url) {
        analyze(&self.client, &self.state, uri).await;
    }

    /// Runs the tests of the program `uri` is in, on the side.
    fn test(&self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        let (client, state) = (self.client.clone(), self.state.clone());
        tokio::spawn(async move { run_tests(&client, &state, root_of(&path)).await });
    }

    /// An analysis of the program `file` is in, with `text` in place of its own: for an
    /// answer that can't wait for the next one. Its diagnostics aren't published.
    async fn analyze_with(&self, file: &str, text: String) -> Option<Analysis> {
        let path = PathBuf::from(file);
        let root = root_of(&path);
        let mut overlay = self.state.overlay().await;
        overlay.insert(path.clone(), text);
        let options = som_driver::Options {
            overlay,
            opened: Some(path),
            every_system: true,
            ..Default::default()
        };
        tokio::task::spawn_blocking(move || som_driver::analyze(&root, &options))
            .await
            .ok()
    }

    /// Analyzes again once typing pauses: unless another change comes first.
    fn analyze_soon(&self, uri: Url, version: i32) {
        let (client, state) = (self.client.clone(), self.state.clone());
        tokio::spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            let Some(file) = file_of(&uri) else {
                return;
            };
            let current = state.documents.read().await.get(&file).map(|(v, _)| *v);
            if current == Some(version) {
                analyze(&client, &state, &uri).await;
            }
        });
    }
}

/// Analyzes the program containing `uri`, with the open files' unsaved text, and publishes
/// its diagnostics. Every program uses the standard library, so a change to it analyzes
/// them all again.
async fn analyze(client: &Client, state: &State, uri: &Url) {
    let Ok(path) = uri.to_file_path() else {
        return;
    };
    let opened = file_of(uri).unwrap_or_default();
    let root = root_of(&path);
    analyze_root(client, state, root.clone(), opened).await;
    if std_root_of(&path).is_some() {
        let others: Vec<PathBuf> = (state.projects.read().await.keys())
            .filter(|r| **r != root)
            .cloned()
            .collect();
        for other in others {
            analyze_root(client, state, other, String::new()).await;
        }
    }
}

/// Analyzes the program at `root` and publishes its diagnostics; `opened` is the file the
/// editor asked about, which gets the errors that have no place of their own.
async fn analyze_root(client: &Client, state: &State, root: PathBuf, opened: String) {
    let _one_at_a_time = state.analyzing.lock().await;

    let options = som_driver::Options {
        overlay: state.overlay().await,
        opened: (!opened.is_empty()).then(|| PathBuf::from(&opened)),
        every_system: true,
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

    let files = loaded_files(&analysis, &opened);
    let mut by_file = diagnostics_by_file(&analysis, &files, &opened);
    let mut projects = state.projects.write().await;
    // Files that had diagnostics and no longer belong to the program get theirs cleared.
    if let Some(previous) = projects.get(&root) {
        for file in previous.files.difference(&files) {
            by_file.entry(file.clone()).or_default();
        }
    }
    let failed = match state.tests.read().await.get(&root) {
        Some(status) => tests::diagnostics(&analysis, status),
        None => HashMap::new(),
    };
    publish(
        client,
        &by_file,
        &failed,
        by_file.keys().chain(failed.keys()),
    )
    .await;
    projects.insert(
        root,
        Project {
            analysis,
            importable,
            files,
            diagnostics: by_file,
        },
    );
    drop(projects);

    // Hints and lenses asked for before this analysis finished are out of date.
    refresh(client, state, true);
}

/// Publishes each of `files`' diagnostics: the analysis's, and the tests' failures.
async fn publish<'a>(
    client: &Client,
    analyzed: &HashMap<String, Vec<Diagnostic>>,
    failed: &HashMap<String, Vec<Diagnostic>>,
    files: impl Iterator<Item = &'a String>,
) {
    let files: HashSet<&String> = files.collect();
    for file in files {
        let Ok(uri) = Url::from_file_path(file) else {
            continue;
        };
        let diagnostics = (analyzed.get(file).into_iter().flatten())
            .chain(failed.get(file).into_iter().flatten())
            .cloned()
            .collect();
        client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

/// Asks the editor for its code lenses again, and with `hints`, its inlay hints. Each
/// refresh waits for the editor's answer, so it's sent on the side.
fn refresh(client: &Client, state: &State, hints: bool) {
    if state.refresh.load(Ordering::Relaxed) {
        let client = client.clone();
        tokio::spawn(async move {
            if hints {
                let _ = client.inlay_hint_refresh().await;
            }
            let _ = client.code_lens_refresh().await;
        });
    }
}

/// Builds and runs the tests of the program at `root`, if it checked without errors and
/// has any, showing them as running until they're done.
async fn run_tests(client: &Client, state: &State, root: PathBuf) {
    let run = state.test_runs.fetch_add(1, Ordering::SeqCst) + 1;
    let _one_at_a_time = state.testing.lock().await;
    if state.test_runs.load(Ordering::SeqCst) != run {
        return;
    }
    let has_tests = (state.projects.read().await.get(&root)).is_some_and(|p| {
        p.analysis.errors.is_empty()
            && !(p.analysis.names.program_tests(&p.analysis.ast)).is_empty()
    });
    if !has_tests {
        // Its results, if any, are of code that's changed.
        // Taken out first: the lock isn't held while waiting for the projects'.
        let before = state.tests.write().await.remove(&root);
        if let Some(before) = before {
            if let Some(project) = state.projects.read().await.get(&root) {
                let gone = tests::diagnostics(&project.analysis, &before);
                publish(client, &project.diagnostics, &HashMap::new(), gone.keys()).await;
            }
            refresh(client, state, true);
        }
        return;
    }
    (state.tests.write().await.entry(root.clone()).or_default()).running = true;
    refresh(client, state, false);

    let options = som_driver::Options {
        overlay: state.overlay().await,
        ..Default::default()
    };
    let tested = root.clone();
    let ran =
        tokio::task::spawn_blocking(move || som_driver::run_tests(&tested, &options, TEST_LIMIT))
            .await;
    let status = match ran {
        Ok(Ok(run)) => TestStatus {
            running: false,
            results: run.results,
            timed_out: run.timed_out.then_some(TEST_LIMIT),
        },
        // It didn't build: nothing's known about its tests.
        _ => TestStatus::default(),
    };
    let before = state
        .tests
        .write()
        .await
        .insert(root.clone(), status.clone());
    // The failures as they were and as they are: those gone are cleared.
    if let Some(project) = state.projects.read().await.get(&root) {
        let failed = tests::diagnostics(&project.analysis, &status);
        let gone = match &before {
            Some(before) => tests::diagnostics(&project.analysis, before),
            None => HashMap::new(),
        };
        let files = failed.keys().chain(gone.keys());
        publish(client, &project.diagnostics, &failed, files).await;
    }
    refresh(client, state, true);
}

/// Every file the program was loaded from, and the opened one.
fn loaded_files(analysis: &Analysis, opened: &str) -> HashSet<String> {
    let ast = &analysis.ast;
    let mut files: HashSet<String> = HashSet::from([opened.to_string()]);
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
    files
}

/// The analysis's warnings and errors, by file, with an empty list for each of `files` that
/// has none so they're cleared.
fn diagnostics_by_file(
    analysis: &Analysis,
    files: &HashSet<String>,
    opened: &str,
) -> HashMap<String, Vec<Diagnostic>> {
    let mut by_file: HashMap<String, Vec<Diagnostic>> =
        files.iter().map(|f| (f.clone(), Vec::new())).collect();
    let diagnostics = (analysis.warnings.iter().map(|w| w.to_diagnostic()))
        .chain(analysis.errors.iter().map(|e| e.to_diagnostic()));
    for diagnostic in diagnostics {
        match convert::som_diagnostic_to_lsp(&diagnostic) {
            Some((file, diagnostics)) => by_file.entry(file).or_default().extend(diagnostics),
            // Errors without a place, like an unreadable file, go on the file that was opened.
            None => by_file
                .entry(opened.to_string())
                .or_default()
                .push(Diagnostic {
                    message: diagnostic.message.plain(),
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("som".into()),
                    ..Default::default()
                }),
        }
    }
    by_file
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
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
        workspace_symbol_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![
                ".".to_string(),
                ":".to_string(),
                "\"".to_string(),
                " ".to_string(),
                "@".to_string(),
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
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            },
        )),
        ..Default::default()
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
        self.state.refresh.store(refresh, Ordering::Relaxed);
        Ok(InitializeResult {
            capabilities: capabilities(),
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
        if let Some(file) = file_of(&document.uri) {
            (self.state.documents.write().await).insert(file, (document.version, document.text));
        }
        self.analyze(&document.uri).await;
        self.test(&document.uri);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // From now on it's read from disk; if it wasn't saved, it's analyzed as it is there.
        if let Some(file) = file_of(&params.text_document.uri) {
            self.state.documents.write().await.remove(&file);
        }
        self.analyze(&params.text_document.uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.analyze(&params.text_document.uri).await;
        self.test(&params.text_document.uri);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // Full sync: the last change is the whole text.
        let (Some(file), Some(change)) = (
            file_of(&params.text_document.uri),
            params.content_changes.into_iter().last(),
        ) else {
            return;
        };
        let version = params.text_document.version;
        (self.state.documents.write().await).insert(file, (version, change.text));
        self.analyze_soon(params.text_document.uri, version);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let at = &params.text_document_position_params;
        Ok(self.at_cursor(at, hover::hover).await)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let at = &params.text_document_position_params;
        Ok(self.at_cursor(at, definition::definition).await)
    }

    async fn goto_declaration(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let at = &params.text_document_position_params;
        Ok(self.at_cursor(at, definition::declaration).await)
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
        let uri = &params.text_document.uri;
        Ok(self
            .in_file(uri, |a, file| Some(definition::document_links(a, file)))
            .await)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let symbols = self
            .in_file(uri, |a, file| Some(symbols::document_symbols(a, file)))
            .await;
        Ok(symbols.map(DocumentSymbolResponse::Nested))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let declarations = params.context.include_declaration;
        let at = &params.text_document_position;
        Ok(self
            .at_cursor(at, |a, cursor| rename::references(a, cursor, declarations))
            .await)
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let at = &params.text_document_position_params;
        Ok(self.at_cursor(at, rename::highlights).await)
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let answer = self.at_cursor(&params, |a, cursor| {
            let Some(uses) = rename::uses_at(a, &cursor.file, cursor.offset) else {
                return Some(Err(Error::invalid_params("there's nothing to rename here")));
            };
            if let Err(why) = rename::renamable(a, &uses) {
                return Some(Err(Error::invalid_params(why)));
            }
            let range = convert::span_to_range(&uses.at.span);
            Some(Ok(PrepareRenameResponse::Range(range)))
        });
        answer.await.transpose()
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let new = params.new_name.trim();
        let answer = self.at_cursor(&params.text_document_position, |a, cursor| {
            let uses = rename::uses_at(a, &cursor.file, cursor.offset)?;
            Some(
                rename::renamable(a, &uses)
                    .and_then(|()| rename::valid_name(new))
                    .map(|()| rename::rename_edit(&uses, new))
                    .map_err(Error::invalid_params),
            )
        });
        answer.await.transpose()
    }

    /// Every function, type and trait in every program whose name matches what's typed.
    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let projects = self.state.projects.read().await;
        let mut seen = HashSet::new();
        let mut found = Vec::new();
        for project in projects.values() {
            for symbol in symbols::workspace_symbols(&project.analysis, &params.query) {
                let start = symbol.location.range.start;
                if seen.insert((symbol.location.uri.to_string(), start.line, start.character)) {
                    found.push(symbol);
                }
            }
        }
        Ok(Some(found))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        let tests = match uri.to_file_path() {
            Ok(path) => self.state.tests.read().await.get(&root_of(&path)).cloned(),
            Err(_) => None,
        };
        Ok(self
            .in_file(uri, |a, file| {
                Some(lens::code_lenses(a, file, uri, tests.as_ref()))
            })
            .await)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        // The editor's text, not the file on disk: unsaved edits would shift every token.
        let Some(file) = file_of(&params.text_document.uri) else {
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
        let at = &params.text_document_position;
        let Some(cursor) = self.cursor(at).await else {
            return Ok(None);
        };
        let projects = self.state.projects.read().await;
        let Some(project) = project_of(&projects, &cursor.file) else {
            return Ok(None);
        };
        // Right after a dot, the last analysis may be from before what's in front of it was
        // typed, since it's only redone once typing pauses. Then it's done now, of the text
        // without the dot and what's typed after it, so what's before it still parses.
        let fresh = match dot_before_word(&cursor.text, cursor.offset) {
            Some(dot)
                if receiver_before(&project.analysis, &cursor.file, &cursor.text, dot)
                    .is_none() =>
            {
                let text = format!("{}{}", &cursor.text[..dot], &cursor.text[cursor.offset..]);
                self.analyze_with(&cursor.file, text).await
            }
            _ => None,
        };
        let request = Request::new(
            fresh.as_ref().unwrap_or(&project.analysis),
            &project.importable,
            &cursor.file,
            &cursor.text,
            cursor.offset,
            at.position,
        );
        let trigger = params.context.and_then(|c| c.trigger_character);
        Ok(completion::complete(&request, trigger.as_deref()).map(CompletionResponse::Array))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let tests = match uri.to_file_path() {
            Ok(path) => self.state.tests.read().await.get(&root_of(&path)).cloned(),
            Err(_) => None,
        };
        Ok(self
            .in_file(uri, |a, file| {
                let mut hints = inlay::inlay_hints(a, file);
                if let Some(tests) = &tests {
                    hints.extend(tests::hints(a, tests, file));
                }
                Some(hints)
            })
            .await)
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let at = &params.text_document_position_params;
        Ok(self.at_cursor(at, signature_help::signature_help).await)
    }
}

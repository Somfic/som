use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use som::arena::Id;
use som::lexer::{TokenKind, lex};
use som::{
    Ast, BorrowChecker, Decl, DefId, Definition, Diagnostic as SomDiagnostic, Expr, FuncKind,
    FuncParam, NameResolver, ProgramLoader, Source, Span, Stmt, Type, TypeInferencer, TypedAst,
};

use crate::convert;
use crate::index::{AstIndex, NodeRef};

/// Result of running the compiler pipeline
pub struct AnalysisResult {
    pub typed_ast: TypedAst,
    pub diagnostics: Vec<SomDiagnostic>,
    pub var_resolutions: HashMap<Id<Expr>, DefId>,
    pub definitions: Vec<Definition>,
}

pub struct SomLanguageServer {
    pub client: Client,
    pub root: RwLock<Option<PathBuf>>,
    pub analyses: RwLock<HashMap<PathBuf, AnalysisResult>>,
}

impl SomLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            root: RwLock::new(None),
            analyses: RwLock::new(HashMap::new()),
        }
    }

    /// Extract the project root directory from a file URI
    fn file_root(uri: &Url) -> Option<PathBuf> {
        let path = uri.to_file_path().ok()?;
        Some(path.parent()?.to_path_buf())
    }

    /// Find the analysis that contains a given file path.
    /// Tries the file's parent directory first (fast path), then searches all analyses.
    fn find_analysis_for_file<'a>(
        analyses: &'a HashMap<PathBuf, AnalysisResult>,
        file_path: &str,
    ) -> Option<&'a AnalysisResult> {
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            if let Some(analysis) = analyses.get(parent) {
                return Some(analysis);
            }
        }
        analyses.values().find(|analysis| {
            AstIndex::find_module_for_file(&analysis.typed_ast.ast, file_path).is_some()
        })
    }

    /// Run the full compiler pipeline and publish diagnostics
    async fn analyze(&self, uri: &Url) {
        let root = match Self::file_root(uri) {
            Some(r) => r,
            None => return,
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!("analyzing with root: {}", root.display()),
            )
            .await;

        let mut all_diagnostics: Vec<SomDiagnostic> = Vec::new();

        // Phase 1: Load and parse
        let loader = ProgramLoader::new(root.clone());
        let ast = match loader.load_project() {
            Ok(ast) => ast,
            Err(errors) => {
                let mut lsp_diags: HashMap<String, Vec<lsp_types::Diagnostic>> = HashMap::new();
                for error in &errors.parse {
                    let diag = error.to_diagnostic();
                    if let Some((file, diags)) = convert::som_diagnostic_to_lsp(&diag) {
                        lsp_diags.entry(file).or_default().extend(diags);
                    }
                }
                for error in &errors.program {
                    let diag = error.to_diagnostic();
                    if let Some((file, diags)) = convert::som_diagnostic_to_lsp(&diag) {
                        lsp_diags.entry(file).or_default().extend(diags);
                    }
                }

                for (file, diags) in lsp_diags {
                    if let Ok(file_uri) = Url::from_file_path(&file) {
                        self.client.publish_diagnostics(file_uri, diags, None).await;
                    }
                }

                self.analyses.write().await.remove(&root);
                return;
            }
        };

        // Phase 2: Name resolution
        let resolved = NameResolver::resolve(ast);
        for error in &resolved.errors {
            all_diagnostics.push(error.to_diagnostic());
        }
        let var_resolutions = resolved.var_resolutions;
        let definitions = resolved.definitions;

        // Phase 3: Type check
        let inferencer = TypeInferencer::new().with_name_resolution();
        let typed_ast = inferencer.check_program(resolved.ast);
        for error in &typed_ast.errors {
            all_diagnostics.push(error.to_diagnostic(&typed_ast.ast));
        }

        // Phase 3: Borrow check
        let mut borrow_checker = BorrowChecker::new(&typed_ast);
        for error in &borrow_checker.check_program() {
            all_diagnostics.push(error.to_diagnostic(&typed_ast));
        }

        // Convert and publish diagnostics grouped by file
        let mut lsp_diags: HashMap<String, Vec<lsp_types::Diagnostic>> = HashMap::new();

        // Initialize empty diagnostics for all known files
        for module in &typed_ast.ast.mods {
            let path = module.path.to_string_lossy().to_string();
            lsp_diags.entry(path).or_default();
        }

        for diag in &all_diagnostics {
            if let Some((file, diags)) = convert::som_diagnostic_to_lsp(diag) {
                lsp_diags.entry(file).or_default().extend(diags);
            }
        }

        for (file, diags) in &lsp_diags {
            if let Ok(file_uri) = Url::from_file_path(file) {
                self.client
                    .publish_diagnostics(file_uri, diags.clone(), None)
                    .await;
            }
        }

        // Store the analysis for hover/definition
        self.analyses.write().await.insert(
            root,
            AnalysisResult {
                typed_ast,
                diagnostics: all_diagnostics,
                var_resolutions,
                definitions,
            },
        );
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SomLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Store workspace root
        if let Some(root_uri) = params.root_uri {
            if let Ok(path) = root_uri.to_file_path() {
                *self.root.write().await = Some(path);
            }
        }

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
                                token_types: vec![
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
                                ],
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

    async fn did_change(&self, _params: DidChangeTextDocumentParams) {
        // We re-analyze on save, not on every keystroke
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        // Read the source to convert position to byte offset
        let source_text = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let offset = match convert::position_to_offset(&source_text, position) {
            Some(o) => o,
            None => return Ok(None),
        };

        // Build index and find node
        let index = AstIndex::build(&analysis.typed_ast.ast);
        let node = match index.find_at(&file_path, offset) {
            Some(n) => n,
            None => return Ok(None),
        };

        // Get type info for the node
        let ast = &analysis.typed_ast.ast;
        let hover_text = match node {
            NodeRef::Expr(expr_id) => {
                let expr = ast.exprs.get(expr_id);
                match expr {
                    // Function call -> show the function's full signature
                    Expr::Call { name, .. } => {
                        let func_name = name.name().value.as_ref();
                        if let Some(func_id) = ast.find_func_by_path(name) {
                            let func = ast.funcs.get(&func_id);
                            format!(
                                "```som\n{}\n```",
                                format_func_signature(
                                    func_name,
                                    &func.parameters,
                                    &func.return_type
                                )
                            )
                        } else if let Some(efunc_id) = ast.find_extern_func_by_name(func_name) {
                            let efunc = ast.extern_funcs.get(&efunc_id);
                            format!(
                                "```som\nextern {}\n```",
                                format_func_signature(
                                    func_name,
                                    &efunc.parameters,
                                    &efunc.return_type
                                )
                            )
                        } else {
                            return Ok(None);
                        }
                    }
                    // Variable reference -> try function signature first, fall back to type
                    Expr::Var(path) => {
                        let name = path.name().value.as_ref();
                        if let Some(func_id) = ast.find_func_by_path(path) {
                            let func = ast.funcs.get(&func_id);
                            format!(
                                "```som\n{}\n```",
                                format_func_signature(name, &func.parameters, &func.return_type)
                            )
                        } else if let Some(efunc_id) = ast.find_extern_func_by_name(name) {
                            let efunc = ast.extern_funcs.get(&efunc_id);
                            format!(
                                "```som\nextern {}\n```",
                                format_func_signature(name, &efunc.parameters, &efunc.return_type)
                            )
                        } else if let Some(struct_id) = ast.find_struct_by_name(name) {
                            let s = ast.structs.get(&struct_id);
                            let fields: Vec<String> = s
                                .fields
                                .iter()
                                .map(|f| format!("    {}: {}", f.name, f.ty))
                                .collect();
                            format!(
                                "```som\nstruct {} {{\n{}\n}}\n```",
                                s.name,
                                fields.join(",\n")
                            )
                        } else {
                            let Some(ty) = analysis.typed_ast.types.get(expr_id) else {
                                return Ok(None);
                            };
                            format!("```som\n{}: {}\n```", path, ty)
                        }
                    }
                    // Other expressions -> show type
                    _ => {
                        let Some(ty) = analysis.typed_ast.types.get(expr_id) else {
                            return Ok(None);
                        };
                        let expr_desc = match expr {
                            Expr::FieldAccess { field, .. } => format!(".{}", field),
                            _ => "expression".to_string(),
                        };
                        format!("```som\n{}: {}\n```", expr_desc, ty)
                    }
                }
            }
            NodeRef::Func(func_id) => {
                let func = ast.funcs.get(func_id);
                format!(
                    "```som\n{}\n```",
                    format_func_signature(&func.name.value, &func.parameters, &func.return_type)
                )
            }
            NodeRef::Struct(struct_id) => {
                let s = ast.structs.get(struct_id);
                let fields: Vec<String> = s
                    .fields
                    .iter()
                    .map(|f| format!("    {}: {}", f.name, f.ty))
                    .collect();
                format!(
                    "```som\nstruct {} {{\n{}\n}}\n```",
                    s.name,
                    fields.join(",\n")
                )
            }
            NodeRef::ExternFunc(efunc_id) => {
                let efunc = ast.extern_funcs.get(efunc_id);
                format!(
                    "```som\nextern {}\n```",
                    format_func_signature(&efunc.name.value, &efunc.parameters, &efunc.return_type)
                )
            }
            _ => return Ok(None),
        };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: hover_text,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        let source_text = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let offset = match convert::position_to_offset(&source_text, position) {
            Some(o) => o,
            None => return Ok(None),
        };

        let index = AstIndex::build(&analysis.typed_ast.ast);
        let node = match index.find_at(&file_path, offset) {
            Some(n) => n,
            None => return Ok(None),
        };

        let span_to_location = |span: &Span| -> Option<Location> {
            let target_file = span.source.identifier();
            let uri = Url::from_file_path(target_file).ok()?;
            let range = convert::span_to_range(span);
            Some(Location { uri, range })
        };

        let target: Option<Location> = match node {
            NodeRef::Expr(expr_id) => {
                let expr = analysis.typed_ast.ast.exprs.get(expr_id);
                match expr {
                    // Variable reference -> find definition
                    Expr::Var(path) => {
                        let name = &*path.name().value;
                        // Try resolved variable (local/parameter) first
                        if let Some(def_id) = analysis.var_resolutions.get(expr_id) {
                            if let Some(def) = analysis.definitions.get(def_id.0 as usize) {
                                // Find the variable name within the statement span
                                // so the cursor lands on the name, not `let`.
                                let span_text = def.span.get_text();
                                let name_offset = span_text.find(&*def.name).unwrap_or(0);
                                let name_start = Span::from_range(
                                    (def.span.start_offset + name_offset)
                                        ..(def.span.start_offset + name_offset + def.name.len()),
                                    def.span.source.clone(),
                                );
                                let uri = Url::from_file_path(def.span.source.identifier()).ok();
                                uri.map(|u| Location {
                                    uri: u,
                                    range: convert::span_to_range(&name_start),
                                })
                            } else {
                                None
                            }
                        // Then try top-level definitions
                        } else if let Some(func_id) = analysis.typed_ast.ast.find_func_by_name(name)
                        {
                            span_to_location(analysis.typed_ast.ast.get_func_span(&func_id))
                        } else if let Some(efunc_id) =
                            analysis.typed_ast.ast.find_extern_func_by_name(name)
                        {
                            span_to_location(analysis.typed_ast.ast.get_extern_func_span(&efunc_id))
                        } else if let Some(struct_id) =
                            analysis.typed_ast.ast.find_struct_by_name(name)
                        {
                            span_to_location(analysis.typed_ast.ast.get_struct_span(&struct_id))
                        } else {
                            None
                        }
                    }
                    // Function call -> go to the function definition
                    Expr::Call { name, .. } => {
                        let func_name = &*name.name().value;
                        if let Some(func_id) = analysis.typed_ast.ast.find_func_by_path(name) {
                            span_to_location(analysis.typed_ast.ast.get_func_span(&func_id))
                        } else if let Some(efunc_id) =
                            analysis.typed_ast.ast.find_extern_func_by_name(func_name)
                        {
                            span_to_location(analysis.typed_ast.ast.get_extern_func_span(&efunc_id))
                        } else {
                            None
                        }
                    }
                    // Constructor -> go to struct definition
                    Expr::Constructor { struct_name, .. } => {
                        let name = &*struct_name.name().value;
                        if let Some(struct_id) = analysis.typed_ast.ast.find_struct_by_name(name) {
                            span_to_location(analysis.typed_ast.ast.get_struct_span(&struct_id))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };

        match target {
            Some(location) => Ok(Some(GotoDefinitionResponse::Scalar(location))),
            None => Ok(None),
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        // Find the module for this file
        let module = match AstIndex::find_module_for_file(&analysis.typed_ast.ast, &file_path) {
            Some(m) => m,
            None => return Ok(None),
        };

        let mut symbols = Vec::new();
        let ast = &analysis.typed_ast.ast;

        for decl in &module.decs {
            match decl {
                som::Decl::Func(func_id) => {
                    let func = ast.funcs.get(func_id);
                    let span = ast.get_func_span(func_id);
                    let range = convert::span_to_range(span);

                    #[allow(deprecated)]
                    symbols.push(DocumentSymbol {
                        name: func.name.value.to_string(),
                        detail: None,
                        kind: SymbolKind::FUNCTION,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: None,
                    });
                }
                som::Decl::Struct(struct_id) => {
                    let s = ast.structs.get(struct_id);
                    let span = ast.get_struct_span(struct_id);
                    let range = convert::span_to_range(span);

                    let children: Vec<DocumentSymbol> = s
                        .fields
                        .iter()
                        .map(|f| {
                            let field_span = ast.get_type_span(&f.type_id);
                            let field_range = convert::span_to_range(field_span);

                            #[allow(deprecated)]
                            DocumentSymbol {
                                name: f.name.value.to_string(),
                                detail: Some(format!("{}", f.ty)),
                                kind: SymbolKind::FIELD,
                                tags: None,
                                deprecated: None,
                                range: field_range,
                                selection_range: field_range,
                                children: None,
                            }
                        })
                        .collect();

                    #[allow(deprecated)]
                    symbols.push(DocumentSymbol {
                        name: s.name.value.to_string(),
                        detail: None,
                        kind: SymbolKind::STRUCT,
                        tags: None,
                        deprecated: None,
                        range,
                        selection_range: range,
                        children: if children.is_empty() {
                            None
                        } else {
                            Some(children)
                        },
                    });
                }
                som::Decl::ExternBlock(block) => {
                    for efunc_id in &block.functions {
                        let efunc = ast.extern_funcs.get(efunc_id);
                        let span = ast.get_extern_func_span(efunc_id);
                        let range = convert::span_to_range(span);

                        #[allow(deprecated)]
                        symbols.push(DocumentSymbol {
                            name: efunc.name.value.to_string(),
                            detail: Some("extern".to_string()),
                            kind: SymbolKind::FUNCTION,
                            tags: None,
                            deprecated: None,
                            range,
                            selection_range: range,
                            children: None,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = &params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        let ast = &analysis.typed_ast.ast;

        // Find the main function (explicit or implicit)
        let main_func_id = ast.find_func_by_name("main");
        let main_func_id = main_func_id.or_else(|| {
            // Check with module-qualified name
            ast.func_registry
                .iter()
                .find(|(name, _)| *name == "main" || name.ends_with("::main"))
                .and_then(|(_, entry)| match &entry.kind {
                    FuncKind::Regular(id) => Some(*id),
                    _ => None,
                })
        });

        let Some(func_id) = main_func_id else {
            return Ok(None);
        };

        let span = ast.get_func_span(&func_id);

        // Only show codelens for files in this project
        if span.source.identifier() != file_path {
            return Ok(None);
        }

        let range = convert::span_to_range(span);
        // Place the lens on the first line of main's span
        let lens_range = Range {
            start: range.start,
            end: lsp_types::Position::new(range.start.line, range.start.character),
        };

        Ok(Some(vec![CodeLens {
            range: lens_range,
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
        let uri = &params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Ok(None),
        };

        let source_text = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let source = Arc::new(
            Source::from_file(&file_path)
                .unwrap_or_else(|_| Source::from_raw(source_text.as_str())),
        );

        let tokens = lex(source);

        let mut semantic_tokens = Vec::new();
        let mut prev_line: u32 = 0;
        let mut prev_start: u32 = 0;

        // Pre-filter to non-whitespace tokens for lookahead, keeping original indices
        let visible: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                !matches!(
                    t.kind,
                    TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error
                )
            })
            .map(|(i, _)| i)
            .collect();

        for (vi, &ti) in visible.iter().enumerate() {
            let token = &tokens[ti];

            let token_type = match token.kind {
                // Keywords
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
                | TokenKind::For => 0, // KEYWORD

                // Identifiers — look ahead to distinguish calls from variables
                TokenKind::Ident => {
                    let next_kind = visible.get(vi + 1).map(|&ni| tokens[ni].kind);
                    if next_kind == Some(TokenKind::OpenParen) {
                        7 // FUNCTION
                    } else if token
                        .span
                        .get_text()
                        .starts_with(|c: char| c.is_uppercase())
                    {
                        8 // STRUCT
                    } else {
                        1 // VARIABLE
                    }
                }

                // Numbers
                TokenKind::Int | TokenKind::Float => 2, // NUMBER

                // Strings
                TokenKind::Text => 3, // STRING

                // Built-in types
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
                | TokenKind::Str => 4, // TYPE

                // Booleans
                TokenKind::True | TokenKind::False => 2, // NUMBER (treat like literals)

                // Comments
                TokenKind::Comment => 5, // COMMENT

                // Star — pointer type after `:` or `->`, otherwise operator
                TokenKind::Star => {
                    let prev_kind = vi
                        .checked_sub(1)
                        .and_then(|pi| visible.get(pi))
                        .map(|&ni| tokens[ni].kind);
                    if matches!(prev_kind, Some(TokenKind::Colon | TokenKind::Arrow)) {
                        4 // TYPE
                    } else {
                        6 // OPERATOR
                    }
                }

                // Operators
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Slash
                | TokenKind::Equals
                | TokenKind::DoubleEquals
                | TokenKind::NotEquals
                | TokenKind::Bang
                | TokenKind::LessThan
                | TokenKind::GreaterThan
                | TokenKind::LessThanOrEqual
                | TokenKind::GreaterThanOrEqual
                | TokenKind::Ampersand => 6, // OPERATOR

                // Delimiters - skip semantic tokens for these
                _ => continue,
            };

            let line = (token.span.start.line.saturating_sub(1)) as u32;
            let start = (token.span.start.col.saturating_sub(1)) as u32;

            let delta_line = line - prev_line;
            let delta_start = if delta_line == 0 {
                start - prev_start
            } else {
                start
            };

            semantic_tokens.push(SemanticToken {
                delta_line,
                delta_start,
                length: token.span.length as u32,
                token_type,
                token_modifiers_bitset: 0,
            });

            prev_line = line;
            prev_start = start;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: semantic_tokens,
        })))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = &params.text_document_position.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        let source_text = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let offset = match convert::position_to_offset(&source_text, position) {
            Some(o) => o,
            None => return Ok(None),
        };

        let ast = &analysis.typed_ast.ast;
        let mut items = Vec::new();

        // Check if we're after a dot (field access completion)
        let after_dot = offset > 0 && source_text.as_bytes().get(offset - 1) == Some(&b'.');

        if after_dot {
            // Find the expression before the dot using AstIndex
            let index = AstIndex::build(ast);
            if let Some(NodeRef::Expr(expr_id)) = index.find_at(&file_path, offset - 2) {
                if let Some(ty) = analysis.typed_ast.types.get(expr_id) {
                    if let Type::Named(name) = ty {
                        if let Some(struct_id) = ast.find_struct_by_name(name.as_ref()) {
                            let s = ast.structs.get(&struct_id);
                            for field in &s.fields {
                                items.push(CompletionItem {
                                    label: field.name.value.to_string(),
                                    kind: Some(CompletionItemKind::FIELD),
                                    detail: Some(format!("{}", field.ty)),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        } else {
            // Functions
            for (name, entry) in &ast.func_registry {
                let label = name.split("::").last().unwrap_or(name).to_string();
                let detail = match &entry.kind {
                    FuncKind::Regular(func_id) => {
                        let func = ast.funcs.get(func_id);
                        format_func_signature(&label, &func.parameters, &func.return_type)
                    }
                    FuncKind::Extern(efunc_id) => {
                        let efunc = ast.extern_funcs.get(efunc_id);
                        format_func_signature(&label, &efunc.parameters, &efunc.return_type)
                    }
                };

                items.push(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::FUNCTION),
                    detail: Some(detail),
                    ..Default::default()
                });
            }

            // Structs
            for s in ast.structs.iter() {
                items.push(CompletionItem {
                    label: s.name.value.to_string(),
                    kind: Some(CompletionItemKind::STRUCT),
                    ..Default::default()
                });
            }

            // Keywords
            for kw in [
                "fn", "let", "mut", "if", "else", "struct", "extern", "loop", "while", "for",
                "use", "true", "false",
            ] {
                items.push(CompletionItem {
                    label: kw.to_string(),
                    kind: Some(CompletionItemKind::KEYWORD),
                    ..Default::default()
                });
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = &params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        let ast = &analysis.typed_ast.ast;
        let module = match AstIndex::find_module_for_file(ast, &file_path) {
            Some(m) => m,
            None => return Ok(None),
        };

        let mut hints = Vec::new();

        for decl in &module.decs {
            if let Decl::Func(func_id) = decl {
                let func = ast.funcs.get(func_id);
                collect_let_hints(ast, &analysis.typed_ast, &func.body, &mut hints);
            }
        }

        Ok(Some(hints))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
        };

        let analyses = self.analyses.read().await;
        let Some(analysis) = Self::find_analysis_for_file(&analyses, &file_path) else {
            return Ok(None);
        };

        let source_text = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };

        let offset = match convert::position_to_offset(&source_text, position) {
            Some(o) => o,
            None => return Ok(None),
        };

        let ast = &analysis.typed_ast.ast;

        // Find the Call expression containing the cursor
        let call_expr_id = match find_containing_call(ast, &file_path, offset) {
            Some(id) => id,
            None => return Ok(None),
        };

        let Expr::Call { name, .. } = ast.exprs.get(&call_expr_id) else {
            return Ok(None);
        };

        let func_name = name.name().value.as_ref();

        let (label, parameters) = if let Some(func_id) = ast.find_func_by_path(name) {
            let func = ast.funcs.get(&func_id);
            build_signature_info(func_name, &func.parameters, &func.return_type)
        } else if let Some(efunc_id) = ast.find_extern_func_by_name(func_name) {
            let efunc = ast.extern_funcs.get(&efunc_id);
            build_signature_info(func_name, &efunc.parameters, &efunc.return_type)
        } else {
            return Ok(None);
        };

        // Determine active parameter by counting commas between ( and cursor
        let call_span = ast.get_expr_span(&call_expr_id);
        let call_end = call_span.start_offset + call_span.length;
        let slice_end = offset.min(call_end);
        let call_text_before_cursor = &source_text[call_span.start_offset..slice_end];
        let active_parameter = if let Some(paren_pos) = call_text_before_cursor.find('(') {
            let after_paren = &call_text_before_cursor[paren_pos + 1..];
            after_paren.chars().filter(|c| *c == ',').count() as u32
        } else {
            0
        };

        Ok(Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: None,
                parameters: Some(parameters),
                active_parameter: Some(active_parameter),
            }],
            active_signature: Some(0),
            active_parameter: Some(active_parameter),
        }))
    }
}

/// Format a function signature string for display
fn format_func_signature(name: &str, params: &[FuncParam], return_type: &Option<Type>) -> String {
    let params_str: Vec<String> = params
        .iter()
        .map(|p| match &p.ty {
            Some(ty) => format!("{}: {}", p.name, ty),
            None => format!("{}", p.name),
        })
        .collect();
    let ret = match return_type {
        Some(ty) => format!(" -> {}", ty),
        None => String::new(),
    };
    format!("fn {}({}){}", name, params_str.join(", "), ret)
}

/// Build signature information for signature help
fn build_signature_info(
    name: &str,
    params: &[FuncParam],
    return_type: &Option<Type>,
) -> (String, Vec<ParameterInformation>) {
    let label = format_func_signature(name, params, return_type);

    let parameters = params
        .iter()
        .map(|p| {
            let param_label = match &p.ty {
                Some(ty) => format!("{}: {}", p.name, ty),
                None => format!("{}", p.name),
            };
            ParameterInformation {
                label: ParameterLabel::Simple(param_label),
                documentation: None,
            }
        })
        .collect();

    (label, parameters)
}

/// Walk a block expression to collect inlay hints for let bindings
fn collect_let_hints(
    ast: &Ast,
    typed_ast: &TypedAst,
    expr_id: &Id<Expr>,
    hints: &mut Vec<InlayHint>,
) {
    if let Expr::Block { stmts, .. } = ast.exprs.get(expr_id) {
        for stmt_id in stmts {
            collect_stmt_hints(ast, typed_ast, stmt_id, hints);
        }
    }
}

/// Recursively collect inlay hints from statements
fn collect_stmt_hints(
    ast: &Ast,
    typed_ast: &TypedAst,
    stmt_id: &Id<Stmt>,
    hints: &mut Vec<InlayHint>,
) {
    let stmt = ast.stmts.get(stmt_id);
    match stmt {
        Stmt::Let {
            name,
            ty: None,
            value,
            ..
        } => {
            if let Some(inferred_ty) = typed_ast.types.get(value) {
                if matches!(inferred_ty, Type::Unit | Type::Unknown(_)) {
                    return;
                }

                let stmt_span = ast.get_stmt_span(stmt_id);
                let stmt_text = stmt_span.get_text();
                let name_str = name.value.as_ref();

                if let Some(name_offset) = stmt_text.find(name_str) {
                    let hint_col = stmt_span.start.col + name_offset + name_str.len();
                    let position = lsp_types::Position::new(
                        (stmt_span.start.line - 1) as u32,
                        (hint_col - 1) as u32,
                    );

                    hints.push(InlayHint {
                        position,
                        label: InlayHintLabel::String(format!(": {}", inferred_ty)),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip: None,
                        padding_left: None,
                        padding_right: None,
                        data: None,
                    });
                }
            }
        }
        Stmt::Loop { body } => {
            for s in body {
                collect_stmt_hints(ast, typed_ast, s, hints);
            }
        }
        Stmt::While { body, .. } => {
            for s in body {
                collect_stmt_hints(ast, typed_ast, s, hints);
            }
        }
        Stmt::Condition {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_stmt_hints(ast, typed_ast, s, hints);
            }
            if let Some(else_stmts) = else_body {
                for s in else_stmts {
                    collect_stmt_hints(ast, typed_ast, s, hints);
                }
            }
        }
        _ => {}
    }
}

/// Find the smallest Call expression containing the given offset
fn find_containing_call(ast: &Ast, file_path: &str, offset: usize) -> Option<Id<Expr>> {
    let mut best: Option<(usize, Id<Expr>)> = None;

    for (expr_id, expr) in ast.exprs.iter_with_ids() {
        if !matches!(expr, Expr::Call { .. }) {
            continue;
        }
        let span = ast.get_expr_span(&expr_id);
        if span.source.identifier() != file_path {
            continue;
        }
        let end = span.start_offset + span.length;
        if offset >= span.start_offset && offset <= end {
            if best.is_none() || span.length < best.unwrap().0 {
                best = Some((span.length, expr_id));
            }
        }
    }

    best.map(|(_, id)| id)
}

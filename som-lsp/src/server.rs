use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use som::lexer::{TokenKind, lex};
use som::{
    BorrowChecker, Diagnostic as SomDiagnostic, Expr, ProgramLoader, Source, Span, TypeInferencer,
    TypedAst,
};

use crate::convert;
use crate::index::{AstIndex, NodeRef};

/// Result of running the compiler pipeline
pub struct AnalysisResult {
    pub typed_ast: TypedAst,
    pub diagnostics: Vec<SomDiagnostic>,
}

pub struct SomLanguageServer {
    pub client: Client,
    pub root: RwLock<Option<PathBuf>>,
    pub analysis: RwLock<Option<AnalysisResult>>,
}

impl SomLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            root: RwLock::new(None),
            analysis: RwLock::new(None),
        }
    }

    /// Run the full compiler pipeline and publish diagnostics
    async fn analyze(&self, uri: &Url) {
        let file_path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => return,
        };

        let root = file_path.parent().unwrap_or(&file_path).to_path_buf();

        self.client
            .log_message(MessageType::INFO, format!("analyzing with root: {}", root.display()))
            .await;

        let mut all_diagnostics: Vec<SomDiagnostic> = Vec::new();

        // Phase 1: Load and parse
        let loader = ProgramLoader::new(root);
        let ast = match loader.load_project() {
            Ok(ast) => ast,
            Err(errors) => {
                let mut lsp_diags: HashMap<String, Vec<lsp_types::Diagnostic>> = HashMap::new();
                for error in &errors {
                    let diag = error.to_diagnostic();
                    if let Some((file, lsp_diag)) = convert::som_diagnostic_to_lsp(&diag) {
                        lsp_diags.entry(file).or_default().push(lsp_diag);
                    }
                }

                for (file, diags) in lsp_diags {
                    if let Ok(file_uri) = Url::from_file_path(&file) {
                        self.client.publish_diagnostics(file_uri, diags, None).await;
                    }
                }

                *self.analysis.write().await = None;
                return;
            }
        };

        // Phase 2: Type check
        let inferencer = TypeInferencer::new();
        let typed_ast = inferencer.check_program(ast);
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
            if let Some((file, lsp_diag)) = convert::som_diagnostic_to_lsp(diag) {
                lsp_diags.entry(file).or_default().push(lsp_diag);
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
        *self.analysis.write().await = Some(AnalysisResult {
            typed_ast,
            diagnostics: all_diagnostics,
        });
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
        let analysis = self.analysis.read().await;
        let Some(analysis) = analysis.as_ref() else {
            return Ok(None);
        };

        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
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
        let hover_text = match node {
            NodeRef::Expr(expr_id) => {
                let expr = analysis.typed_ast.ast.exprs.get(expr_id);
                match analysis.typed_ast.types.get(expr_id) {
                    Some(ty) => {
                        let expr_desc = match expr {
                            Expr::Var(path) => format!("{}", path),
                            Expr::Call { name, .. } => format!("{}(...)", name),
                            Expr::FieldAccess { field, .. } => {
                                format!(".{}", field)
                            }
                            _ => "expression".to_string(),
                        };
                        format!("```som\n{}: {}\n```", expr_desc, ty)
                    }
                    None => return Ok(None),
                }
            }
            NodeRef::Func(func_id) => {
                let func = analysis.typed_ast.ast.funcs.get(func_id);
                let params: Vec<String> = func
                    .parameters
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{}: {}", p.name, ty),
                        None => format!("{}", p.name),
                    })
                    .collect();
                let ret = match &func.return_type {
                    Some(ty) => format!(" -> {}", ty),
                    None => String::new(),
                };
                format!(
                    "```som\nfn {}({}){}\n```",
                    func.name,
                    params.join(", "),
                    ret
                )
            }
            NodeRef::Struct(struct_id) => {
                let s = analysis.typed_ast.ast.structs.get(struct_id);
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
                let efunc = analysis.typed_ast.ast.extern_funcs.get(efunc_id);
                let params: Vec<String> = efunc
                    .parameters
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{}: {}", p.name, ty),
                        None => format!("{}", p.name),
                    })
                    .collect();
                let ret = match &efunc.return_type {
                    Some(ty) => format!(" -> {}", ty),
                    None => String::new(),
                };
                format!(
                    "```som\nextern fn {}({}){}\n```",
                    efunc.name,
                    params.join(", "),
                    ret
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
        let analysis = self.analysis.read().await;
        let Some(analysis) = analysis.as_ref() else {
            return Ok(None);
        };

        let uri = &params.text_document_position_params.text_document.uri;
        let position = &params.text_document_position_params.position;

        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
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

        let target_span: Option<&Span> = match node {
            NodeRef::Expr(expr_id) => {
                let expr = analysis.typed_ast.ast.exprs.get(expr_id);
                match expr {
                    // Variable reference -> find the function it refers to
                    Expr::Var(path) => {
                        let name = &*path.name().value;
                        // Try to find as function first
                        if let Some(func_id) = analysis.typed_ast.ast.find_func_by_name(name) {
                            Some(analysis.typed_ast.ast.get_func_span(&func_id))
                        } else if let Some(efunc_id) =
                            analysis.typed_ast.ast.find_extern_func_by_name(name)
                        {
                            Some(analysis.typed_ast.ast.get_extern_func_span(&efunc_id))
                        } else if let Some(struct_id) =
                            analysis.typed_ast.ast.find_struct_by_name(name)
                        {
                            Some(analysis.typed_ast.ast.get_struct_span(&struct_id))
                        } else {
                            None
                        }
                    }
                    // Function call -> go to the function definition
                    Expr::Call { name, .. } => {
                        let func_name = &*name.name().value;
                        if let Some(func_id) = analysis.typed_ast.ast.find_func_by_path(name) {
                            Some(analysis.typed_ast.ast.get_func_span(&func_id))
                        } else if let Some(efunc_id) =
                            analysis.typed_ast.ast.find_extern_func_by_name(func_name)
                        {
                            Some(analysis.typed_ast.ast.get_extern_func_span(&efunc_id))
                        } else {
                            None
                        }
                    }
                    // Constructor -> go to struct definition
                    Expr::Constructor { struct_name, .. } => {
                        let name = &*struct_name.name().value;
                        if let Some(struct_id) = analysis.typed_ast.ast.find_struct_by_name(name) {
                            Some(analysis.typed_ast.ast.get_struct_span(&struct_id))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };

        match target_span {
            Some(span) => {
                let target_file = span.source.identifier();
                let target_uri = match Url::from_file_path(target_file) {
                    Ok(u) => u,
                    Err(_) => return Ok(None),
                };

                let range = convert::span_to_range(span);
                Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range,
                })))
            }
            None => Ok(None),
        }
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let analysis = self.analysis.read().await;
        let Some(analysis) = analysis.as_ref() else {
            return Ok(None);
        };

        let uri = &params.text_document.uri;
        let file_path = match uri.to_file_path() {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => return Ok(None),
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

        for token in &tokens {
            if matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::Eof | TokenKind::Error
            ) {
                continue;
            }

            let token_type = match token.kind {
                // Keywords
                TokenKind::Fn
                | TokenKind::Extern
                | TokenKind::Struct
                | TokenKind::Let
                | TokenKind::If
                | TokenKind::Else
                | TokenKind::Mut
                | TokenKind::Use
                | TokenKind::Loop
                | TokenKind::While
                | TokenKind::For => 0, // KEYWORD

                // Identifiers
                TokenKind::Ident => 1, // VARIABLE

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

                // Operators
                TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
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
}

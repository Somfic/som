use std::collections::HashMap;

use som_ast::{TokenKind, lex};
use som_common::Id;
use tokio::sync::RwLock;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, jsonrpc::Result};

use crate::check::check;
use crate::convert::{self, LineIndex};

/// Semantic-token legend. The order here defines the numeric ids sent on the
/// wire, and must stay in sync with the `semanticTokenScopes` map in the VS
/// Code extension's `package.json`.
const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,  // 0
    SemanticTokenType::VARIABLE, // 1
    SemanticTokenType::NUMBER,   // 2
    SemanticTokenType::STRING,   // 3
    SemanticTokenType::TYPE,     // 4
    SemanticTokenType::COMMENT,  // 5
    SemanticTokenType::OPERATOR, // 6
];

/// Map a lexer token to a semantic-token type id, or `None` to leave it to the
/// TextMate grammar (punctuation, delimiters, EOF, …).
fn token_type(kind: TokenKind) -> Option<u32> {
    use TokenKind::*;
    Some(match kind {
        Fn | Extern | Struct | Impl | Let | If | Else | Mut | Use | Loop | While | For | True
        | False => 0,
        I8 | I16 | I32 | I64 | I128 | ISize | U8 | U16 | U32 | U64 | U128 | USize | F32 | F64
        | Bool | Char | Str => 4,
        Int | Float => 2,
        Text => 3,
        Comment => 5,
        Plus | Minus | Star | Slash | Equals | DoubleEquals | NotEquals | Or | And | Bang
        | LessThan | GreaterThan | LessThanOrEquals | GreaterThanOrEquals | Percentage | Arrow
        | FatArrow | Ampersand => 6,
        Ident => 1,
        _ => return None,
    })
}

pub struct SomLanguageServer {
    client: Client,
    /// Latest text of every open document, keyed by URI.
    documents: RwLock<HashMap<Url, String>>,
}

impl SomLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: RwLock::new(HashMap::new()),
        }
    }

    /// Check-compile `text` and publish the resulting diagnostics for `uri`.
    async fn analyze(&self, uri: Url, text: &str, version: Option<i32>) {
        let index = LineIndex::new(text);
        let diagnostics = check(text)
            .iter()
            .map(|d| convert::to_lsp(&uri, &index, d))
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, version)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SomLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "som-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                        legend: SemanticTokensLegend {
                            token_types: TOKEN_TYPES.to_vec(),
                            token_modifiers: vec![],
                        },
                        full: Some(SemanticTokensFullOptions::Bool(true)),
                        range: None,
                        work_done_progress_options: Default::default(),
                    }),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "som language server ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.documents
            .write()
            .await
            .insert(doc.uri.clone(), doc.text.clone());
        self.analyze(doc.uri, &doc.text, Some(doc.version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // FULL sync: the last change carries the whole document.
        let Some(change) = params.content_changes.into_iter().last() else {
            return;
        };
        let uri = params.text_document.uri;
        self.documents
            .write()
            .await
            .insert(uri.clone(), change.text.clone());
        self.analyze(uri, &change.text, Some(params.text_document.version))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.remove(&uri);
        // Clear diagnostics for the closed file.
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(text) = self.documents.read().await.get(&uri).cloned() else {
            return Ok(None);
        };

        let index = LineIndex::new(&text);
        let tokens = lex(Id::new(0), &text);

        let mut data: Vec<SemanticToken> = Vec::new();
        let mut prev_line = 0u32;
        let mut prev_start = 0u32;

        for token in tokens {
            let Some(token_type) = token_type(token.kind) else {
                continue;
            };
            let start = index.position(token.span.start as usize);
            let end = index.position(token.span.end as usize);
            // Semantic tokens can't straddle lines; skip anything that does.
            if start.line != end.line || end.character < start.character {
                continue;
            }
            let length = end.character - start.character;
            if length == 0 {
                continue;
            }

            let delta_line = start.line - prev_line;
            let delta_start = if delta_line == 0 {
                start.character - prev_start
            } else {
                start.character
            };
            data.push(SemanticToken {
                delta_line,
                delta_start,
                length,
                token_type,
                token_modifiers_bitset: 0,
            });
            prev_line = start.line;
            prev_start = start.character;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }
}

//! Language server for som, served over stdio. The `som` binary starts it with
//! `som lsp`.
//!
//! It check-compiles each open document on change and publishes diagnostics,
//! and serves lexer-derived semantic tokens for highlighting. Richer features
//! (hover, go-to-definition) need name/type introspection the compiler doesn't
//! expose yet.

mod check;
mod convert;
mod server;

use server::SomLanguageServer;
use tower_lsp::{LspService, Server};

/// Start the language server on stdio and block until the client disconnects.
pub fn run() -> std::io::Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(SomLanguageServer::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(())
}

//! The som language server, started by editors as `som lsp`. It speaks the
//! Language Server Protocol over stdin and stdout.

mod convert;
mod index;
mod server;

use server::SomLanguageServer;
use tower_lsp::{LspService, Server};

/// Serves one editor session over stdin and stdout, returning when it ends.
pub fn serve() {
    let runtime = tokio::runtime::Runtime::new().expect("could not start the async runtime");
    runtime.block_on(async {
        let (service, socket) = LspService::new(SomLanguageServer::new);
        Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
            .serve(service)
            .await;
    });
}

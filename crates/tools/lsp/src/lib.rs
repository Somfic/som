//! The som language server, started by editors as `som lsp`. It speaks the
//! Language Server Protocol over stdin and stdout.

mod convert;
mod docs;
mod handlers;
mod index;
mod lookup;
mod origin;
mod references;
mod server;
#[cfg(test)]
mod test_support;
mod text_scan;
mod types;

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

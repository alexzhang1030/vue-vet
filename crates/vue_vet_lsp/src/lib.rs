//! Thin LSP adapter over [`vue_vet_session`].
//!
//! Publishes on-disk diagnostics with the same opaque finding ids as CLI JSON.
//! Document overlays, code actions, and cancellation are deferred.

mod convert;
mod server;

pub use convert::{span_to_range, to_lsp_diagnostic};
pub use server::Backend;

use tower_lsp::{LspService, Server};

/// Run the language server on stdin/stdout until the client shuts down.
///
/// # Errors
///
/// Returns an I/O error when the Tokio runtime cannot start.
pub fn run_stdio() -> std::io::Result<()> {
  let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
  runtime.block_on(async {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
  });
  Ok(())
}

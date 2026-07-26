//! Thin LSP adapter over [`vue_vet_session`].
//!
//! Publishes diagnostics for on-disk files and unsaved buffer overlays with the
//! same opaque finding ids as CLI JSON, and offers explicitly safe quick-fix
//! code actions as versioned workspace edits. Request-level cancellation and
//! MCP remain later issue #12 work.

mod convert;
mod server;

pub use convert::{
  SafeCodeActionRequest, byte_range_to_range, safe_code_actions, span_to_range, to_lsp_diagnostic,
};
pub use server::{Backend, is_current_generation};

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

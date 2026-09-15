//! Thin MCP adapter over [`vue_vet_session`].
//!
//! Newline-delimited JSON-RPC 2.0 over stdio (one message per line) exposing
//! scan, explain, explain-scope, and safe-fix preview tools. The live server
//! keeps one [`vue_vet_session::ProjectSession`] per resolved tool path: scan /
//! preview replace it (disk edits stay visible); explain / explain-scope reuse
//! the last committed snapshot. Apply remains CLI / LSP — never silent.

mod protocol;
mod tools;

pub use protocol::{McpServer, read_message, write_message};
pub use tools::{TOOL_NAMES, call_tool, list_tools};

use std::{io::BufReader, path::PathBuf};

/// Run the MCP server on stdin/stdout until the client closes the stream.
///
/// # Errors
///
/// Returns an I/O error. Tool failures are returned as MCP
/// tool results, not as process-level errors.
pub fn run_stdio(workspace_root: PathBuf) -> std::io::Result<()> {
  let server = McpServer::new(workspace_root);
  let stdin = std::io::stdin();
  let stdout = std::io::stdout();
  let mut reader = BufReader::new(stdin.lock());
  let mut writer = stdout.lock();
  server.serve(&mut reader, &mut writer)
}

//! Thin MCP adapter over [`vue_vet_session`].
//!
//! Stdio JSON-RPC (Content-Length framing) exposing scan, explain, explain-scope,
//! and safe-fix preview tools. Analysis stays in the session; this crate only
//! maps protocol and enforces workspace path bounds. Apply remains CLI / LSP —
//! never silent.

mod protocol;
mod tools;

pub use protocol::{McpServer, read_message, write_message};
pub use tools::{TOOL_NAMES, call_tool, list_tools};

use std::{
  io::{BufReader, Write},
  path::PathBuf,
};

/// Run the MCP server on stdin/stdout until the client closes the stream.
///
/// # Errors
///
/// Returns an I/O or protocol framing error. Tool failures are returned as MCP
/// tool results, not as process-level errors.
pub fn run_stdio(workspace_root: PathBuf) -> std::io::Result<()> {
  let server = McpServer::new(workspace_root);
  let stdin = std::io::stdin();
  let stdout = std::io::stdout();
  let mut reader = BufReader::new(stdin.lock());
  let mut writer = stdout.lock();
  while let Some(message) = read_message(&mut reader)? {
    if let Some(response) = server.handle(&message) {
      write_message(&mut writer, &response)?;
      writer.flush()?;
    }
  }
  Ok(())
}

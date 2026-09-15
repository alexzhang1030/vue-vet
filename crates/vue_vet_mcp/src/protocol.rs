//! Minimal MCP tools-subset over newline-delimited JSON-RPC 2.0 (one message per line).

use std::{
  io::{BufRead, Write},
  path::PathBuf,
  sync::Mutex,
};

use serde_json::{Value, json};

use crate::tools::{McpSessionSlot, call_tool_on, list_tools};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "vue-vet";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateful MCP server bound to one workspace root.
///
/// Scan / preview replace the bound [`vue_vet_session::ProjectSession`] so a
/// later disk edit is visible. Explain / explain-scope reuse that session's
/// last committed snapshot (same idea as LSP hover after a scan).
#[derive(Debug)]
pub struct McpServer {
  workspace_root: PathBuf,
  session: Mutex<McpSessionSlot>,
}

impl McpServer {
  /// Bind tools to `workspace_root`. Relative roots are absolutized against the
  /// process cwd so lexical workspace checks stay meaningful.
  #[must_use]
  pub fn new(workspace_root: PathBuf) -> Self {
    let absolute = if workspace_root.is_absolute() {
      workspace_root
    } else {
      match std::env::current_dir() {
        Ok(cwd) => cwd.join(workspace_root),
        Err(_) => workspace_root,
      }
    };
    let workspace_root = absolute.canonicalize().unwrap_or(absolute);
    Self { workspace_root, session: Mutex::new(None) }
  }

  #[must_use]
  pub fn workspace_root(&self) -> &std::path::Path {
    &self.workspace_root
  }

  /// Handle one JSON-RPC message. Returns `None` for notifications.
  #[must_use]
  pub fn handle(&self, message: &Value) -> Option<Value> {
    let method = message.get("method").and_then(Value::as_str);
    let id = message.get("id").cloned();
    match method {
      Some("initialize") => Some(rpc_result(id.as_ref(), initialize_result())),
      Some("notifications/initialized" | "initialized") => None,
      Some("ping") => Some(rpc_result(id.as_ref(), json!({}))),
      Some("tools/list") => Some(rpc_result(id.as_ref(), json!({ "tools": list_tools() }))),
      Some("tools/call") => {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let result = self.session.lock().map_or_else(
          |_| crate::tools::tool_error_message("session lock poisoned"),
          |mut slot| call_tool_on(&self.workspace_root, &mut slot, name, &arguments),
        );
        Some(rpc_result(id.as_ref(), result))
      }
      Some(unknown) => {
        id.as_ref()?;
        Some(rpc_error(id.as_ref(), -32601, &format!("Method not found: {unknown}")))
      }
      None => {
        id.as_ref()?;
        Some(rpc_error(id.as_ref(), -32600, "Invalid Request: missing method"))
      }
    }
  }

  /// Read newline-delimited JSON-RPC from `reader` until EOF.
  ///
  /// Empty lines are skipped. A malformed JSON line writes a `-32700` parse
  /// error (`id: null`) and the loop continues.
  ///
  /// # Errors
  ///
  /// Returns I/O or JSON encode failures. Parse errors stay on the stream.
  pub(crate) fn serve(
    &self,
    reader: &mut impl BufRead,
    writer: &mut impl Write,
  ) -> std::io::Result<()> {
    loop {
      match read_message(reader) {
        Ok(None) => return Ok(()),
        Ok(Some(message)) => {
          if let Some(response) = self.handle(&message) {
            write_message(writer, &response)?;
          }
        }
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
          write_message(writer, &rpc_error(None, -32700, "Parse error"))?;
        }
        Err(error) => return Err(error),
      }
    }
  }
}

fn initialize_result() -> Value {
  json!({
    "protocolVersion": PROTOCOL_VERSION,
    "capabilities": {
      "tools": {}
    },
    "serverInfo": {
      "name": SERVER_NAME,
      "version": SERVER_VERSION
    },
    "instructions": "Vue Vet MCP tools scan, explain rules/findings, explain tracking scopes (would Vue re-run?), and preview safe fixes inside the bound workspace. Fixes are never applied through MCP."
  })
}

fn rpc_result(id: Option<&Value>, result: Value) -> Value {
  let mut object = serde_json::Map::new();
  object.insert("jsonrpc".into(), Value::String("2.0".into()));
  object.insert("id".into(), id.cloned().unwrap_or(Value::Null));
  object.insert("result".into(), result);
  Value::Object(object)
}

fn rpc_error(id: Option<&Value>, code: i64, message: &str) -> Value {
  let mut object = serde_json::Map::new();
  object.insert("jsonrpc".into(), Value::String("2.0".into()));
  object.insert("id".into(), id.cloned().unwrap_or(Value::Null));
  object.insert(
    "error".into(),
    json!({
      "code": code,
      "message": message
    }),
  );
  Value::Object(object)
}

/// Read the next newline-delimited JSON-RPC message from `reader`.
///
/// Empty lines are skipped. `Ok(None)` means EOF.
///
/// # Errors
///
/// Returns I/O errors, or `ErrorKind::InvalidData` when a non-empty line is not JSON.
pub fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
  for line in reader.lines() {
    let line = line?;
    if line.is_empty() {
      continue;
    }
    return serde_json::from_str(&line).map(Some).map_err(|error| {
      std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid JSON: {error}"))
    });
  }
  Ok(None)
}

/// Write one newline-delimited JSON-RPC message to `writer`.
///
/// # Errors
///
/// Returns I/O or JSON encode failures.
pub fn write_message(writer: &mut impl Write, message: &Value) -> std::io::Result<()> {
  serde_json::to_writer(&mut *writer, message).map_err(|error| {
    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode JSON: {error}"))
  })?;
  writer.write_all(b"\n")?;
  writer.flush()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Cursor;

  #[test]
  #[expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "framing fixture failures must fail the unit test"
  )]
  fn round_trips_newline_delimited_framing() {
    let message = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    let mut buffer = Vec::new();
    write_message(&mut buffer, &message).unwrap_or_else(|_| panic!("write"));
    assert!(buffer.ends_with(b"\n"), "each message is one newline-terminated line");
    assert!(
      !buffer.starts_with(b"Content-Length"),
      "stdio MCP must not use LSP headers: {}",
      String::from_utf8_lossy(&buffer)
    );
    let mut reader = Cursor::new(buffer);
    let Ok(Some(decoded)) = read_message(&mut reader) else {
      panic!("read");
    };
    assert_eq!(decoded, message);

    let server = McpServer::new(PathBuf::from("."));
    let input = concat!(
      "\n\n",
      r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
      "\nnot-json\n",
      r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
      "\n"
    );
    let mut stdout = Vec::new();
    server
      .serve(&mut Cursor::new(input.as_bytes()), &mut stdout)
      .unwrap_or_else(|_| panic!("serve"));
    let lines = stdout
      .split(|byte| *byte == b'\n')
      .filter(|line| !line.is_empty())
      .map(|line| serde_json::from_slice::<Value>(line).unwrap_or_else(|_| panic!("json line")))
      .collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["id"], 1);
    assert_eq!(lines[0]["result"], json!({}));
    assert_eq!(lines[1]["id"], Value::Null);
    assert_eq!(lines[1]["error"]["code"], -32700);
    assert_eq!(lines[2]["id"], 2);
  }

  #[test]
  #[expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "initialize response shape is fixed for this unit test"
  )]
  fn initialize_advertises_tools_capability() {
    let server = McpServer::new(PathBuf::from("."));
    let Some(response) =
      server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
    else {
      panic!("response");
    };
    assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
    assert!(response["result"]["capabilities"]["tools"].is_object());
  }

  fn placeholder_workspace() -> (PathBuf, String) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
      .join("../../fixtures/rules/no-computed-without-dependency/invalid/placeholder.vue");
    let workspace = root.parent().map_or_else(|| PathBuf::from("."), PathBuf::from);
    let file = root
      .file_name()
      .map_or_else(|| "placeholder.vue".into(), |name| name.to_string_lossy().into_owned());
    (workspace, file)
  }

  fn call_named(server: &McpServer, id: u64, name: &str, arguments: &Value) -> Value {
    server
      .handle(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
      }))
      .unwrap_or_else(|| json!({"error": "no response"}))
  }

  fn bound_committed_analyses(server: &McpServer) -> u64 {
    server
      .session
      .lock()
      .ok()
      .and_then(|slot| slot.as_ref().map(|(_, session)| session.stats().committed_analyses))
      .unwrap_or(0)
  }

  #[test]
  #[expect(clippy::indexing_slicing, reason = "tool result shape is fixed for this unit test")]
  fn explain_scope_reuses_scan_session() {
    let (workspace, file) = placeholder_workspace();
    let server = McpServer::new(workspace);
    let scan = call_named(&server, 1, "vue_vet_scan", &json!({ "path": file }));
    assert_eq!(scan["result"]["isError"], false, "{scan}");
    let explain =
      call_named(&server, 2, "vue_vet_explain_scope", &json!({ "query": "label", "path": file }));
    assert_eq!(explain["result"]["isError"], false, "{explain}");
    assert_eq!(
      bound_committed_analyses(&server),
      1,
      "explain-scope must reuse the scan snapshot instead of opening a new session"
    );
  }

  #[test]
  #[expect(clippy::indexing_slicing, reason = "tool result shape is fixed for this unit test")]
  fn second_explain_scope_does_not_reanalyze() {
    let (workspace, file) = placeholder_workspace();
    let server = McpServer::new(workspace);
    let first =
      call_named(&server, 1, "vue_vet_explain_scope", &json!({ "query": "label", "path": file }));
    assert_eq!(first["result"]["isError"], false, "{first}");
    let second =
      call_named(&server, 2, "vue_vet_explain_scope", &json!({ "query": "label", "path": file }));
    assert_eq!(second["result"]["isError"], false, "{second}");
    assert_eq!(
      bound_committed_analyses(&server),
      1,
      "a second explain-scope on the same path must not re-enter analyze"
    );
  }

  #[test]
  #[expect(
    clippy::indexing_slicing,
    clippy::panic,
    reason = "tool result shape is fixed for this unit test"
  )]
  fn scan_replaces_session_so_disk_edits_are_visible() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-mcp-session-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    let _ignored = std::fs::remove_dir_all(&dir);
    assert!(std::fs::create_dir_all(&dir).is_ok(), "temp dir");
    let path = dir.join("App.vue");
    assert!(
      std::fs::write(&path, "<template><div v-html=\"x\" /></template>\n").is_ok(),
      "write dirty"
    );
    let server = McpServer::new(dir.clone());
    let first = call_named(&server, 1, "vue_vet_scan", &json!({ "path": "App.vue" }));
    assert_eq!(first["result"]["isError"], false, "{first}");
    let first_text = first["result"]["content"][0]["text"].as_str().unwrap_or_default();
    let Ok(first_report) = serde_json::from_str::<Value>(first_text) else {
      let _ignored = std::fs::remove_dir_all(&dir);
      panic!("first scan must return JSON: {first_text}");
    };
    assert!(
      first_report["diagnostics"].as_array().is_some_and(|items| !items.is_empty()),
      "v-html fixture must emit a finding: {first_report}"
    );
    assert!(std::fs::write(&path, "<template><div /></template>\n").is_ok(), "write clean");
    let second = call_named(&server, 2, "vue_vet_scan", &json!({ "path": "App.vue" }));
    assert_eq!(second["result"]["isError"], false, "{second}");
    let second_text = second["result"]["content"][0]["text"].as_str().unwrap_or_default();
    let Ok(report) = serde_json::from_str::<Value>(second_text) else {
      let _ignored = std::fs::remove_dir_all(&dir);
      panic!("second scan must return JSON: {second_text}");
    };
    let diagnostics = report["diagnostics"].as_array().map_or(0, Vec::len);
    assert_eq!(diagnostics, 0, "replaced session must see the cleaned file: {report}");
    let _ignored = std::fs::remove_dir_all(&dir);
  }
}

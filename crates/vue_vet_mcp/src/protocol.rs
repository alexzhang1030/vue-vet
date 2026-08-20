//! Minimal MCP tools-subset over JSON-RPC 2.0 with LSP-style Content-Length framing.

use std::{
  io::{BufRead, Write},
  path::PathBuf,
};

use serde_json::{Value, json};

use crate::tools::{call_tool, list_tools};

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "vue-vet";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Stateful MCP server bound to one workspace root.
#[derive(Debug)]
pub struct McpServer {
  workspace_root: PathBuf,
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
    Self { workspace_root }
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
        let result = call_tool(&self.workspace_root, name, &arguments);
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

/// Read one Content-Length framed JSON message from `reader`.
///
/// # Errors
///
/// Returns I/O errors, invalid framing, or JSON decode failures. `Ok(None)` means EOF
/// before a message starts.
pub fn read_message(reader: &mut impl BufRead) -> std::io::Result<Option<Value>> {
  let mut content_length = None;
  loop {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
      return Ok(None);
    }
    let trimmed = line.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
      break;
    }
    if let Some(value) = trimmed.strip_prefix("Content-Length:") {
      let parsed = value.trim().parse::<usize>().map_err(|error| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Content-Length: {error}"))
      })?;
      content_length = Some(parsed);
    }
  }
  let Some(length) = content_length else {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidData,
      "missing required Content-Length header",
    ));
  };
  let mut body = vec![0_u8; length];
  reader.read_exact(&mut body)?;
  serde_json::from_slice(&body).map(Some).map_err(|error| {
    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("invalid JSON body: {error}"))
  })
}

/// Write one Content-Length framed JSON message to `writer`.
///
/// # Errors
///
/// Returns I/O or JSON encode failures.
pub fn write_message(writer: &mut impl Write, message: &Value) -> std::io::Result<()> {
  let body = serde_json::to_vec(message).map_err(|error| {
    std::io::Error::new(std::io::ErrorKind::InvalidData, format!("encode JSON: {error}"))
  })?;
  write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
  writer.write_all(&body)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Cursor;

  #[test]
  #[expect(clippy::panic, reason = "framing fixture failures must fail the unit test")]
  fn round_trips_content_length_framing() {
    let message = json!({"jsonrpc":"2.0","id":1,"method":"ping"});
    let mut buffer = Vec::new();
    write_message(&mut buffer, &message).unwrap_or_else(|_| panic!("write"));
    let mut reader = Cursor::new(buffer);
    let Ok(Some(decoded)) = read_message(&mut reader) else {
      panic!("read");
    };
    assert_eq!(decoded, message);
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
}

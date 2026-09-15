use std::{
  io::Write,
  process::{Command, Stdio},
};

use super::helpers::*;
use serde_json::json;
use vue_vet_mcp::TOOL_NAMES;

#[test]
#[expect(
  clippy::indexing_slicing,
  clippy::panic,
  reason = "MCP stdio fixture failures must fail the integration test"
)]
fn mcp_stdio_round_trips_initialize_and_tools_list() {
  let mut child = match Command::new(env!("CARGO_BIN_EXE_vue-vet"))
    .arg("--mcp")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
  {
    Ok(child) => child,
    Err(error) => panic!("failed to spawn vue-vet --mcp: {error}"),
  };
  let Some(mut stdin) = child.stdin.take() else {
    panic!("vue-vet --mcp must expose stdin");
  };
  let initialize = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
      "protocolVersion": "2024-11-05",
      "capabilities": {},
      "clientInfo": { "name": "vue-vet-test", "version": "0" }
    }
  });
  let initialized = json!({
    "jsonrpc": "2.0",
    "method": "notifications/initialized"
  });
  let tools_list = json!({
    "jsonrpc": "2.0",
    "id": 2,
    "method": "tools/list"
  });
  if writeln!(stdin, "{initialize}").is_err()
    || writeln!(stdin, "{initialized}").is_err()
    || writeln!(stdin, "{tools_list}").is_err()
  {
    panic!("failed to write MCP requests to vue-vet --mcp");
  }
  drop(stdin);

  let output = match child.wait_with_output() {
    Ok(output) => output,
    Err(error) => panic!("vue-vet --mcp did not exit: {error}"),
  };
  let stdout = String::from_utf8_lossy(&output.stdout);
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(output.status.success(), "vue-vet --mcp must exit 0: {stderr}{stdout}");

  let messages = stdout
    .lines()
    .map(|line| match serde_json::from_str::<Value>(line) {
      Ok(value) => value,
      Err(error) => panic!("each stdout line must be JSON ({error}): {line}"),
    })
    .collect::<Vec<_>>();
  assert_eq!(messages.len(), 2, "initialize + tools/list; notification is silent: {stdout}");
  assert_eq!(
    messages[0]["result"]["protocolVersion"].as_str(),
    Some("2024-11-05"),
    "initialize must advertise MCP 2024-11-05: {}",
    messages[0]
  );
  let names = messages[1]["result"]["tools"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|tool| tool.get("name").and_then(Value::as_str))
    .collect::<Vec<_>>();
  assert_eq!(names, TOOL_NAMES, "tools/list must expose the crate tool names: {}", messages[1]);
}

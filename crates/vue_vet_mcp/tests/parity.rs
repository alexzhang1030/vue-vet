use std::{
  fs,
  path::{Path, PathBuf},
  sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::{Value, json};
use vue_vet_mcp::{TOOL_NAMES, call_tool};
use vue_vet_reporters::report_diagnostic_id;
use vue_vet_session::{ProjectSession, SessionOptions};

fn fixture(name: &str) -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures").join(name)
}

#[test]
#[expect(
  clippy::indexing_slicing,
  clippy::panic,
  reason = "parity test indexes known MCP tool result shape"
)]
fn mcp_scan_finding_ids_match_session() {
  let root = fixture("rules/no-v-html/invalid/basic.vue");
  let workspace = root.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
  let file_name =
    root.file_name().map_or_else(|| "basic.vue".into(), |name| name.to_string_lossy().into_owned());

  let Ok(session) = ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir: None,
    no_cache: true,
    threads: Some(1),
  }) else {
    panic!("session must open");
  };
  let Ok(snapshot) = session.analyze() else {
    panic!("analyze must succeed");
  };
  assert!(!snapshot.summary.diagnostics.is_empty(), "fixture must emit findings");
  let expected = snapshot
    .summary
    .diagnostics
    .iter()
    .map(|diagnostic| report_diagnostic_id(diagnostic, &snapshot.analyzed_files))
    .collect::<Vec<_>>();

  let result = call_tool(&workspace, "vue_vet_scan", &json!({ "path": file_name }));
  assert_eq!(result["isError"], false, "{result}");
  let text = result["content"][0]["text"].as_str().unwrap_or_default();
  let Ok(report) = serde_json::from_str::<Value>(text) else {
    panic!("scan tool must return JSON: {text}");
  };
  let ids = report["diagnostics"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|diagnostic| diagnostic.get("id").and_then(Value::as_str).map(str::to_owned))
    .collect::<Vec<_>>();
  assert_eq!(ids, expected);
}

#[test]
#[expect(clippy::indexing_slicing, reason = "parity test indexes known MCP tool result shape")]
fn mcp_explain_rule_returns_docs() {
  let workspace = fixture("rules/no-v-html");
  let result =
    call_tool(&workspace, "vue_vet_explain", &json!({ "target": "vue-vet/security/no-v-html" }));
  assert_eq!(result["isError"], false, "{result}");
  let text = result["content"][0]["text"].as_str().unwrap_or_default();
  assert!(text.contains("vue-vet/security/no-v-html"), "{text}");
}

#[test]
#[expect(
  clippy::indexing_slicing,
  clippy::panic,
  reason = "parity test indexes known MCP tool result shape"
)]
fn mcp_preview_safe_fixes_never_writes() {
  static NEXT: AtomicUsize = AtomicUsize::new(0);
  let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
  let dir =
    std::env::temp_dir().join(format!("vue-vet-mcp-preview-{}-{sequence}", std::process::id()));
  let _ignored = fs::remove_dir_all(&dir);
  assert!(fs::create_dir_all(&dir).is_ok(), "temp dir");
  let path = dir.join("App.vue");
  let source = "<template><div autofocus /></template>\n";
  assert!(fs::write(&path, source).is_ok(), "write fixture");

  let result = call_tool(&dir, "vue_vet_preview_safe_fixes", &json!({ "path": "." }));
  assert_eq!(result["isError"], false, "{result}");
  let text = result["content"][0]["text"].as_str().unwrap_or_default();
  let Ok(preview) = serde_json::from_str::<Value>(text) else {
    panic!("preview must return JSON: {text}");
  };
  assert_eq!(preview["applied"], false);
  assert_eq!(preview["ok"], true);

  let Ok(after_source) = fs::read_to_string(&path) else {
    panic!("read after preview");
  };
  assert_eq!(after_source, source, "preview must not mutate source");
  let _ignored = fs::remove_dir_all(&dir);
}

#[test]
fn tool_names_are_stable() {
  assert_eq!(TOOL_NAMES.len(), 3);
  assert!(TOOL_NAMES.contains(&"vue_vet_scan"));
  assert!(TOOL_NAMES.contains(&"vue_vet_explain"));
  assert!(TOOL_NAMES.contains(&"vue_vet_preview_safe_fixes"));
}

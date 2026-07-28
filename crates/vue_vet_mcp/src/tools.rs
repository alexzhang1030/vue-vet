//! MCP tool handlers over [`vue_vet_session`].

use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

use serde_json::{Value, json};
use vue_vet_core::{EditApplicability, EditPlan, TextEdit};
use vue_vet_reporters::{
  ReportContext, ReportFormat, ReportFramework, ReportMode, render, render_finding_explain_json,
  render_rule_explain_json,
};
use vue_vet_session::{
  AnalysisSnapshot, Explained, ProjectSession, SessionOptions, resolve_under_root, scan_directory,
};

/// Stable tool names for docs and tests.
pub const TOOL_NAMES: &[&str] = &["vue_vet_scan", "vue_vet_explain", "vue_vet_preview_safe_fixes"];

#[must_use]
pub fn list_tools() -> Vec<Value> {
  vec![
    tool_descriptor(
      "vue_vet_scan",
      "Scan a path inside the workspace and return the Vue Vet JSON v1 report (same identities as CLI `--format json`).",
      &json!({
        "type": "object",
        "properties": {
          "path": {
            "type": "string",
            "description": "Workspace-relative file or directory to scan. Defaults to the workspace root."
          }
        },
        "additionalProperties": false
      }),
    ),
    tool_descriptor(
      "vue_vet_explain",
      "Explain a rule id or opaque finding id (same payload as CLI `--explain`). Finding ids require a prior scan of the same path.",
      &json!({
        "type": "object",
        "properties": {
          "target": {
            "type": "string",
            "description": "Full rule id (for example `vue-vet/security/no-v-html`) or finding id from JSON/LSP."
          },
          "path": {
            "type": "string",
            "description": "Workspace-relative scan root used when explaining a finding id. Defaults to the workspace root."
          }
        },
        "required": ["target"],
        "additionalProperties": false
      }),
    ),
    tool_descriptor(
      "vue_vet_preview_safe_fixes",
      "Validate and preview explicitly safe edit candidates without writing files. Unsafe edits are never included; apply remains CLI `--fix-safe` or LSP code actions.",
      &json!({
        "type": "object",
        "properties": {
          "path": {
            "type": "string",
            "description": "Workspace-relative file or directory to scan. Defaults to the workspace root."
          }
        },
        "additionalProperties": false
      }),
    ),
  ]
}

/// Dispatch a tool call into a MCP `tools/call` result object.
#[must_use]
pub fn call_tool(workspace_root: &Path, name: &str, arguments: &Value) -> Value {
  match name {
    "vue_vet_scan" => match tool_scan(workspace_root, arguments) {
      Ok(text) => tool_success(&text),
      Err(error) => tool_error(error),
    },
    "vue_vet_explain" => match tool_explain(workspace_root, arguments) {
      Ok(text) => tool_success(&text),
      Err(error) => tool_error(error),
    },
    "vue_vet_preview_safe_fixes" => match tool_preview_safe_fixes(workspace_root, arguments) {
      Ok(text) => tool_success(&text),
      Err(error) => tool_error(error),
    },
    other => tool_error(format!("unknown tool `{other}`")),
  }
}

fn tool_scan(workspace_root: &Path, arguments: &Value) -> Result<String, String> {
  let target = resolve_tool_path(workspace_root, arguments)?;
  let session = open_session(&target)?;
  let snapshot = session.analyze().map_err(|error| error.to_string())?;
  let context = report_context(&target, &snapshot);
  render(&snapshot.summary, ReportFormat::Json, &context).map_err(|error| error.to_string())
}

fn tool_explain(workspace_root: &Path, arguments: &Value) -> Result<String, String> {
  let target = arguments
    .get("target")
    .and_then(Value::as_str)
    .filter(|value| !value.is_empty())
    .ok_or_else(|| "vue_vet_explain requires a non-empty `target`".to_owned())?;
  let path = resolve_tool_path(workspace_root, arguments)?;
  let session = open_session(&path)?;
  match session.explain(target).map_err(|error| error.to_string())? {
    Explained::Rule(explain) => {
      render_rule_explain_json(&explain).map_err(|error| error.to_string())
    }
    Explained::Finding { explain, .. } => {
      render_finding_explain_json(&explain).map_err(|error| error.to_string())
    }
  }
}

fn tool_preview_safe_fixes(workspace_root: &Path, arguments: &Value) -> Result<String, String> {
  let target = resolve_tool_path(workspace_root, arguments)?;
  let session = open_session(&target)?;
  let snapshot = session.analyze().map_err(|error| error.to_string())?;
  let boundary = session.workspace_root();
  let mut safe_edits = Vec::new();
  for diagnostic in &snapshot.summary.diagnostics {
    for edit in &diagnostic.edits {
      if edit.applicability != EditApplicability::Safe {
        continue;
      }
      let _validated_file = resolve_edit_file(boundary, edit.file.as_path())?;
      safe_edits.push(TextEdit {
        file: edit.file.clone(),
        range: edit.range,
        replacement: edit.replacement.clone(),
        applicability: EditApplicability::Safe,
        rule_id: edit.rule_id.clone(),
      });
    }
  }
  let plan = EditPlan::new(safe_edits).map_err(|error| error.to_string())?;
  let mut files = plan
    .edits()
    .iter()
    .map(|edit| normalize_display_path(boundary, edit.file.as_path()))
    .collect::<Vec<_>>();
  files.sort();
  files.dedup();
  let edits = plan
    .edits()
    .iter()
    .map(|edit| {
      json!({
        "file": normalize_display_path(boundary, edit.file.as_path()),
        "rule_id": edit.rule_id,
        "offset": edit.range.offset,
        "length": edit.range.length,
        "replacement": edit.replacement,
        "applicability": "safe"
      })
    })
    .collect::<Vec<_>>();
  serde_json::to_string_pretty(&json!({
    "schema_version": 1,
    "ok": true,
    "applied": false,
    "edit_count": plan.edits().len(),
    "file_count": files.len(),
    "files": files,
    "edits": edits
  }))
  .map_err(|error| error.to_string())
}

fn resolve_tool_path(workspace_root: &Path, arguments: &Value) -> Result<PathBuf, String> {
  let relative = arguments.get("path").and_then(Value::as_str).unwrap_or(".");
  resolve_under_root(workspace_root, Path::new(relative)).map_err(|error| error.to_string())
}

fn resolve_edit_file(workspace_root: &Path, file: &Path) -> Result<PathBuf, String> {
  resolve_under_root(workspace_root, file).map_err(|error| error.to_string())
}

fn open_session(root: &Path) -> Result<ProjectSession, String> {
  ProjectSession::open(SessionOptions {
    root: root.to_path_buf(),
    config_path: None,
    cache_dir: None,
    no_cache: false,
    threads: None,
  })
  .map_err(|error| error.to_string())
}

fn report_context(path: &Path, snapshot: &AnalysisSnapshot) -> ReportContext {
  let mut skipped_check_reasons = BTreeMap::new();
  if let Some(error) = &snapshot.graph.reactivity_error {
    skipped_check_reasons.insert("module_reactivity".into(), error.clone());
  }
  for (index, issue) in snapshot.issues.iter().enumerate() {
    skipped_check_reasons
      .entry(format!("analysis_{index}"))
      .or_insert_with(|| issue.message.clone());
  }
  let project_root = {
    let root = scan_directory(path).to_string_lossy().replace('\\', "/");
    if root.is_empty() { ".".into() } else { root }
  };
  ReportContext {
    mode: ReportMode::Full,
    framework: detect_framework(path),
    project_root,
    analyzed_files: snapshot.analyzed_files.clone(),
    complete: snapshot.complete(),
    skipped_check_reasons,
    reactivity: None,
    component_nav: None,
  }
}

fn detect_framework(root: &Path) -> ReportFramework {
  let package = if root.is_dir() {
    root.join("package.json")
  } else {
    root.parent().unwrap_or(root).join("package.json")
  };
  let Ok(source) = fs::read_to_string(package) else {
    return ReportFramework::Vue;
  };
  let Ok(package) = serde_json::from_str::<Value>(&source) else {
    return ReportFramework::Vue;
  };
  let is_nuxt = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
    .iter()
    .filter_map(|section| package.get(*section))
    .any(|section| section.get("nuxt").is_some());
  if is_nuxt { ReportFramework::Nuxt } else { ReportFramework::Vue }
}

fn normalize_display_path(root: &Path, path: &Path) -> String {
  path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn tool_descriptor(name: &str, description: &str, input_schema: &Value) -> Value {
  json!({
    "name": name,
    "description": description,
    "inputSchema": input_schema
  })
}

fn tool_success(text: &str) -> Value {
  json!({
    "content": [{ "type": "text", "text": text }],
    "isError": false
  })
}

fn tool_error(message: impl Into<String>) -> Value {
  json!({
    "content": [{ "type": "text", "text": message.into() }],
    "isError": true
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[expect(clippy::indexing_slicing, reason = "unit test indexes known tool-error shape")]
  fn rejects_workspace_escape() {
    let root = PathBuf::from("/workspace");
    let result = call_tool(&root, "vue_vet_scan", &json!({ "path": "../secret" }));
    assert_eq!(result["isError"], true);
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("escapes"), "{text}");
  }

  #[test]
  fn lists_expected_tools() {
    let names = list_tools()
      .into_iter()
      .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_owned))
      .collect::<Vec<_>>();
    assert_eq!(names, TOOL_NAMES);
  }
}

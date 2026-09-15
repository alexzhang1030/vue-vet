//! One data-driven walker over `fixtures/rules/<rule>/` through the session entry.
//!
//! Invalid fixtures compare post-overlap diagnostics to `fixtures/snapshots/<rule>/<stem>.json`.
//! Set `UPDATE_RULE_SNAPSHOTS=1` to refresh those files.
//!
//! Companion files (imported by another fixture in the same directory, not the
//! trigger) use a `PascalCase` stem (`Child.vue`, `LiteralChild.vue`). They are
//! scanned and snapshotted but are not required to report the target rule.

#![expect(clippy::panic, reason = "fixture IO or analysis errors must fail the walker")]

use std::{
  collections::{BTreeMap, BTreeSet},
  fs, io,
  path::{Path, PathBuf},
};

use vue_vet_core::{Diagnostic, FileId, LineIndex};

use super::helpers::*;

/// Rule fixture directories the walker must not treat as single-file rules.
const SKIPPED_RULE_DIRS: &[(&str, &str)] = &[
  (
    "recommended",
    "legacy pack; golden.rs::recommended_rule_pack_covers_all_rules_with_valid_spans",
  ),
  (
    "no-stale-prop-flow",
    "dead until #273: join_prop_flows only puts Prop edges on the child graph; fixtures kept",
  ),
  (
    "unresolved-import",
    "no fixtures/rules dir; pipeline_tests/graph.rs::reports_broken_imports_and_unused_components",
  ),
  (
    "unused-component",
    "no fixtures/rules dir; pipeline_tests/graph.rs::reports_broken_imports_and_unused_components",
  ),
  ("vapor-assessment", "no fixtures/rules dir; vapor_migration.rs::matched_complete_file_is_ready"),
  (
    "vapor-sfc-compile-contract",
    "no fixtures/rules dir; vapor_migration.rs::matched_sfc_compile_contract_reasons",
  ),
  (
    "vapor-memo-contract-dropped",
    "no fixtures/rules dir; vapor_migration.rs::matched_memo_spans_and_unicode_line_column",
  ),
  (
    "vapor-interop-required",
    "no fixtures/rules dir; vapor_migration.rs::matched_interop_and_unresolved_child",
  ),
  (
    "vapor-runtime-envelope",
    "no fixtures/rules dir; vapor_migration.rs::envelope_reports_each_untested_construct_and_stays_convertible",
  ),
  (
    "no-nonreactive-props-destructure",
    "Vue <3.5 only; golden.rs::recommended_rule_pack_covers_all_rules_with_valid_spans",
  ),
  (
    "no-model-default-unsynced-parent-demand",
    "parent+child join; model_demand.rs::rule_fixtures_match_reporter_snapshots",
  ),
  (
    "no-shared-default-cross-instance-demand",
    "parent+child join; model_demand.rs::rule_fixtures_match_reporter_snapshots",
  ),
];

const SOURCE_EXTENSIONS: &[&str] = &["vue", "ts", "tsx"];

#[test]
fn rule_fixture_walker() {
  let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
  let rules_root = repo.join("fixtures/rules");
  let snapshots_root = repo.join("fixtures/snapshots");
  let update = std::env::var_os("UPDATE_RULE_SNAPSHOTS").is_some();
  let skipped: BTreeSet<&str> = SKIPPED_RULE_DIRS.iter().map(|(name, _)| *name).collect();
  let mut expected_snapshots = BTreeSet::new();
  let mut failures = Vec::new();
  let mut dirs: Vec<PathBuf> = fs::read_dir(&rules_root)
    .unwrap_or_else(|error| panic!("read {}: {error}", rules_root.display()))
    .map(|entry| entry.unwrap_or_else(|error| panic!("entry: {error}")).path())
    .filter(|path| path.is_dir())
    .collect();
  dirs.sort();
  for rule_dir in dirs {
    let rule = rule_dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if skipped.contains(rule) {
      continue;
    }
    walk_rule(&rule_dir, rule, &snapshots_root, update, &mut expected_snapshots, &mut failures);
  }
  if update {
    prune_stale_snapshots(&snapshots_root, &expected_snapshots, &skipped);
  }
  assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn walk_rule(
  rule_dir: &Path,
  rule: &str,
  snapshots_root: &Path,
  update: bool,
  expected_snapshots: &mut BTreeSet<PathBuf>,
  failures: &mut Vec<String>,
) {
  let invalid_dir = rule_dir.join("invalid");
  let valid_dir = rule_dir.join("valid");
  let invalid_files = list_sources(&invalid_dir);
  let valid_files = list_sources(&valid_dir);
  assert!(!invalid_files.is_empty(), "{rule} must have at least one invalid source fixture");
  require_unicode_and_crlf(rule, &invalid_dir, &invalid_files);

  let workspace = prepare_workspace(rule_dir);
  let session = open_session_threads(workspace.clone(), 1);
  let snapshot = session.analyze().unwrap_or_else(|error| panic!("analyze {rule}: {error}"));
  let mut by_scan_file: BTreeMap<String, Vec<Diagnostic>> = BTreeMap::new();
  for diagnostic in snapshot.summary.diagnostics.iter().cloned() {
    by_scan_file.entry(diagnostic.file.as_str().to_owned()).or_default().push(diagnostic);
  }

  let snap_dir = snapshots_root.join(rule);
  if update {
    fs::create_dir_all(&snap_dir)
      .unwrap_or_else(|error| panic!("mkdir {}: {error}", snap_dir.display()));
  }

  for path in &invalid_files {
    let snap_name = snapshot_file_name(path);
    let relative = path.strip_prefix(rule_dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
    let logical = format!("fixtures/rules/{rule}/{relative}");
    let source =
      fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let mut diagnostics = by_scan_file.remove(&relative).unwrap_or_default();
    for diagnostic in &mut diagnostics {
      diagnostic.file = FileId::from(logical.as_str());
    }
    if diagnostics.iter().all(|diagnostic| !diagnostic.rule_id.ends_with(&format!("/{rule}"))) {
      if is_companion_fixture(path) {
        // PascalCase stem: imported sibling, not the trigger file.
      } else {
        let ids: Vec<&str> =
          diagnostics.iter().map(|diagnostic| diagnostic.rule_id.as_str()).collect();
        failures.push(format!("{logical} must report {rule}; ids={ids:?}"));
        continue;
      }
    }
    assert_spans_and_positions(&logical, &source, &diagnostics);
    let actual = serde_json::to_string_pretty(&diagnostics)
      .unwrap_or_else(|error| panic!("serialize {logical}: {error}"));
    let snap_path = snap_dir.join(snap_name);
    expected_snapshots.insert(snap_path.clone());
    if update {
      fs::write(&snap_path, format!("{actual}\n"))
        .unwrap_or_else(|error| panic!("write {}: {error}", snap_path.display()));
    }
    let expected = fs::read_to_string(&snap_path)
      .unwrap_or_else(|error| panic!("missing snapshot {}: {error}", snap_path.display()));
    assert_eq!(actual, expected.trim_end(), "snapshot changed for {logical}");
  }

  for path in &valid_files {
    let relative = path.strip_prefix(rule_dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
    let logical = format!("fixtures/rules/{rule}/{relative}");
    let diagnostics = by_scan_file.remove(&relative).unwrap_or_default();
    if diagnostics.iter().any(|diagnostic| diagnostic.rule_id.ends_with(&format!("/{rule}"))) {
      failures.push(format!("{logical} must stay quiet for {rule}; {diagnostics:?}"));
    }
  }

  let _ignored = fs::remove_dir_all(workspace);
}

fn require_unicode_and_crlf(rule: &str, invalid_dir: &Path, files: &[PathBuf]) {
  let names: Vec<String> = files
    .iter()
    .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(str::to_owned))
    .collect();
  assert!(
    names.iter().any(|name| name.starts_with("unicode") && is_vue_name(name)),
    "{rule} must have invalid/unicode*.vue"
  );
  let crlf = names.iter().find(|name| name.starts_with("crlf") && is_vue_name(name));
  let Some(crlf_name) = crlf else {
    panic!("{rule} must have invalid/crlf*.vue");
  };
  let bytes = fs::read(invalid_dir.join(crlf_name))
    .unwrap_or_else(|error| panic!("read {rule} {crlf_name}: {error}"));
  assert!(
    bytes.windows(2).any(|window| window == b"\r\n"),
    "{rule}/{crlf_name} must contain CRLF bytes"
  );
}

fn assert_spans_and_positions(logical: &str, source: &str, diagnostics: &[Diagnostic]) {
  let index = LineIndex::new(source);
  for diagnostic in diagnostics {
    let end = diagnostic.span.offset.saturating_add(diagnostic.span.length);
    assert!(
      source.get(diagnostic.span.offset..end).is_some_and(|snippet| !snippet.is_empty()),
      "{logical} span must be a non-empty slice of the original source ({})",
      diagnostic.rule_id
    );
    let (line, column) = index.byte_to_line_column(diagnostic.span.offset);
    assert_eq!(
      (diagnostic.span.line, diagnostic.span.column),
      (line, column),
      "{logical} line/column must round-trip from byte offset ({})",
      diagnostic.rule_id
    );
  }
}

fn snapshot_file_name(path: &Path) -> String {
  let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("fixture");
  match path.extension().and_then(|ext| ext.to_str()) {
    Some("vue") | None => format!("{stem}.json"),
    Some(ext) => format!("{stem}.{ext}.json"),
  }
}

fn is_vue_name(name: &str) -> bool {
  Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("vue"))
}

/// `Child.vue` / `LiteralChild.vue`: first stem character is uppercase ASCII.
fn is_companion_fixture(path: &Path) -> bool {
  path
    .file_stem()
    .and_then(|stem| stem.to_str())
    .is_some_and(|stem| stem.starts_with(|first: char| first.is_uppercase()))
}

fn list_sources(dir: &Path) -> Vec<PathBuf> {
  if !dir.is_dir() {
    return Vec::new();
  }
  let mut files: Vec<PathBuf> = fs::read_dir(dir)
    .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    .map(|entry| entry.unwrap_or_else(|error| panic!("entry: {error}")).path())
    .filter(|path| {
      path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext))
    })
    .collect();
  files.sort();
  files
}

fn prepare_workspace(rule_dir: &Path) -> PathBuf {
  let workspace = std::env::temp_dir().join(format!(
    "vue-vet-rule-fixtures-{}-{}",
    rule_dir.file_name().and_then(|name| name.to_str()).unwrap_or("rule"),
    std::process::id()
  ));
  let _ignored = fs::remove_dir_all(&workspace);
  fs::create_dir_all(&workspace).unwrap_or_else(|error| panic!("workspace: {error}"));
  copy_dir(rule_dir, &workspace).unwrap_or_else(|error| panic!("copy fixtures: {error}"));
  install_scan_packages(&workspace);
  fs::write(
    workspace.join("vue-vet.toml"),
    "version = 1\n\n[rules]\n\"vue-vet/project/unresolved-import\" = \"off\"\n\"vue-vet/project/unused-component\" = \"off\"\n",
  )
  .unwrap_or_else(|error| panic!("vue-vet.toml: {error}"));
  workspace
}

fn install_scan_packages(root: &Path) {
  install_module_seeds_vue_stub(root);
  for (name, version) in [
    ("@vueuse/core", "13.9.0"),
    ("@vueuse/shared", "13.9.0"),
    ("pinia", "2.3.0"),
    ("vue-router", "4.5.0"),
  ] {
    let dest = root.join("node_modules").join(name);
    fs::create_dir_all(&dest).unwrap_or_else(|error| panic!("{name} dir: {error}"));
    fs::write(
      dest.join("package.json"),
      format!(
        "{{\"name\":\"{name}\",\"version\":\"{version}\",\"main\":\"index.js\",\"exports\":{{\".\":\"index.js\"}}}}\n"
      ),
    )
    .unwrap_or_else(|error| panic!("{name} package.json: {error}"));
    fs::write(dest.join("index.js"), "export {}\n")
      .unwrap_or_else(|error| panic!("{name} index: {error}"));
  }
  fs::write(
    root.join("package.json"),
    "{\n  \"dependencies\": {\n    \"vue\": \"3.5.0\",\n    \"@vueuse/core\": \"13.9.0\",\n    \"@vueuse/shared\": \"13.9.0\",\n    \"pinia\": \"2.3.0\",\n    \"vue-router\": \"4.5.0\"\n  }\n}\n",
  )
  .unwrap_or_else(|error| panic!("package.json: {error}"));
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
  fs::create_dir_all(to)?;
  for entry in fs::read_dir(from)? {
    let entry = entry?;
    let source = entry.path();
    let dest = to.join(entry.file_name());
    if source.is_dir() {
      copy_dir(&source, &dest)?;
    } else {
      fs::copy(&source, &dest)?;
    }
  }
  Ok(())
}

fn prune_stale_snapshots(
  snapshots_root: &Path,
  expected: &BTreeSet<PathBuf>,
  skipped: &BTreeSet<&str>,
) {
  if !snapshots_root.is_dir() {
    return;
  }
  for rule_dir in fs::read_dir(snapshots_root)
    .unwrap_or_else(|error| panic!("read snapshots: {error}"))
    .filter_map(Result::ok)
  {
    let path = rule_dir.path();
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
    if !path.is_dir() || name == "parser" || skipped.contains(name) {
      continue;
    }
    for file in
      fs::read_dir(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    {
      let file = file.unwrap_or_else(|error| panic!("snapshot entry: {error}")).path();
      if file.extension().and_then(|ext| ext.to_str()) == Some("json") && !expected.contains(&file)
      {
        fs::remove_file(&file).unwrap_or_else(|error| panic!("remove {}: {error}", file.display()));
      }
    }
  }
}

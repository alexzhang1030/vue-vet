//! Machine-checked analysis-stack compatibility matrix (#13).

use std::{collections::BTreeMap, fs, path::PathBuf};

use serde::Deserialize;
use vue_vet_cache::{AnalysisStackIdentity, CACHE_OXC_PARSER_VERSION, CACHE_VIZE_CROQUIS_VERSION};

#[derive(Debug, Deserialize)]
struct CompatMatrix {
  schema_version: u8,
  rust_channel: String,
  rust_version_msrv: String,
  workspace_pins: BTreeMap<String, String>,
  vue_fixture_ranges: BTreeMap<String, String>,
}

fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(clippy::panic, reason = "compat fixtures must load or the gate fails")]
fn load_matrix() -> CompatMatrix {
  let path = workspace_root().join("fixtures/quality/compat-matrix.json");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let Ok(matrix) = serde_json::from_str(&source) else {
    panic!("parse {}", path.display());
  };
  matrix
}

#[expect(clippy::panic, reason = "toolchain must declare a channel")]
fn toolchain_channel() -> String {
  let path = workspace_root().join("rust-toolchain.toml");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  for line in source.lines() {
    let trimmed = line.trim();
    if let Some(value) = trimmed.strip_prefix("channel") {
      let value = value.trim().trim_start_matches('=').trim();
      return value.trim_matches('"').to_owned();
    }
  }
  panic!("channel missing in rust-toolchain.toml");
}

#[expect(clippy::panic, reason = "workspace must declare rust-version")]
fn workspace_msrv() -> String {
  let path = workspace_root().join("Cargo.toml");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let mut in_package = false;
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with('[') {
      in_package = trimmed == "[workspace.package]";
      continue;
    }
    if in_package && let Some(value) = trimmed.strip_prefix("rust-version") {
      let value = value.trim().trim_start_matches('=').trim();
      return value.trim_matches('"').to_owned();
    }
  }
  panic!("rust-version missing in [workspace.package]");
}

#[expect(clippy::panic, reason = "workspace pins must be exact")]
fn workspace_pin(name: &str) -> String {
  let path = workspace_root().join("Cargo.toml");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let mut in_deps = false;
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with('[') {
      in_deps = trimmed == "[workspace.dependencies]";
      continue;
    }
    if !in_deps {
      continue;
    }
    let Some((key, value)) = trimmed.split_once('=') else {
      continue;
    };
    if key.trim() != name {
      continue;
    }
    let value = value.trim();
    if let Some(exact) = value.strip_prefix("\"=").and_then(|rest| rest.strip_suffix('"')) {
      return exact.to_owned();
    }
    if let Some(rest) = value.split_once("version = \"=").map(|(_, rest)| rest)
      && let Some((exact, _)) = rest.split_once('"')
    {
      return exact.to_owned();
    }
  }
  panic!("workspace pin missing for {name}");
}

#[expect(clippy::panic, reason = "lockfile must contain pinned packages")]
fn lockfile_version(name: &str) -> String {
  let path = workspace_root().join("Cargo.lock");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let mut current_name = None::<String>;
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed == "[[package]]" {
      current_name = None;
      continue;
    }
    if let Some(value) = trimmed.strip_prefix("name = \"") {
      current_name = Some(value.trim_end_matches('"').to_owned());
      continue;
    }
    if current_name.as_deref() == Some(name)
      && let Some(value) = trimmed.strip_prefix("version = \"")
    {
      return value.trim_end_matches('"').to_owned();
    }
  }
  panic!("Cargo.lock missing package {name}");
}

#[expect(clippy::panic, reason = "Vue fixtures must declare dependencies.vue")]
fn package_json_vue_range(project: &str) -> String {
  let path = workspace_root().join(project).join("package.json");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let Ok(value) = serde_json::from_str::<serde_json::Value>(&source) else {
    panic!("parse {}", path.display());
  };
  value
    .pointer("/dependencies/vue")
    .and_then(serde_json::Value::as_str)
    .map_or_else(|| panic!("vue dependency missing in {project}"), str::to_owned)
}

#[test]
#[expect(clippy::panic, reason = "compat gate fails closed when a required pin is missing")]
fn compat_matrix_matches_toolchain_workspace_and_lockfile() {
  let matrix = load_matrix();
  assert_eq!(matrix.schema_version, 1);
  assert_eq!(matrix.rust_channel, toolchain_channel());
  assert_eq!(matrix.rust_version_msrv, workspace_msrv());
  assert!(
    matrix.rust_channel.starts_with(&matrix.rust_version_msrv),
    "channel {} must satisfy MSRV {}",
    matrix.rust_channel,
    matrix.rust_version_msrv
  );

  for (name, expected) in &matrix.workspace_pins {
    assert_eq!(workspace_pin(name), *expected, "Cargo.toml pin drift for {name}");
    assert_eq!(lockfile_version(name), *expected, "Cargo.lock drift for {name}");
  }

  for (project, expected) in &matrix.vue_fixture_ranges {
    assert_eq!(package_json_vue_range(project), *expected, "Vue fixture range drift for {project}");
  }

  let identity = AnalysisStackIdentity::current();
  assert_eq!(identity.vize_croquis, CACHE_VIZE_CROQUIS_VERSION);
  assert_eq!(identity.oxc_parser, CACHE_OXC_PARSER_VERSION);
  let Some(matrix_vize) = matrix.workspace_pins.get("vize_croquis") else {
    panic!("compat-matrix missing vize_croquis");
  };
  let Some(matrix_oxc) = matrix.workspace_pins.get("oxc_parser") else {
    panic!("compat-matrix missing oxc_parser");
  };
  assert_eq!(
    identity.vize_croquis, matrix_vize,
    "content_key vize identity must match compat-matrix pin"
  );
  assert_eq!(
    identity.oxc_parser, matrix_oxc,
    "content_key oxc identity must match compat-matrix pin"
  );
  assert_eq!(identity.vize_croquis, workspace_pin("vize_croquis"));
  assert_eq!(identity.oxc_parser, workspace_pin("oxc_parser"));
  assert_eq!(identity.vize_croquis, lockfile_version("vize_croquis"));
  assert_eq!(identity.oxc_parser, lockfile_version("oxc_parser"));
  assert_eq!(identity.oxc_resolver, lockfile_version("oxc_resolver"));
}

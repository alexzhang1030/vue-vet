//! Quality corpus integrity, precision expectations, and cold/warm identity (#13).

use std::{
  collections::BTreeSet,
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};
use vue_vet_reporters::report_diagnostic_id;
use vue_vet_session::{ProjectSession, SessionOptions};

#[derive(Debug, Deserialize)]
struct Manifest {
  schema_version: u8,
  projects: Vec<ManifestProject>,
}

#[derive(Debug, Deserialize)]
struct ManifestProject {
  id: String,
  path: String,
  roles: Vec<String>,
  tree_digest: String,
}

#[derive(Debug, Deserialize)]
struct PrecisionFile {
  project: String,
  findings: Vec<PrecisionFinding>,
}

#[derive(Debug, Deserialize)]
struct PrecisionFinding {
  rule_id: String,
  file: String,
  classification: String,
}

fn workspace_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[expect(clippy::panic, reason = "quality fixtures must load or the gate fails")]
fn load_manifest() -> Manifest {
  let path = workspace_root().join("fixtures/quality/manifest.json");
  let Ok(source) = fs::read_to_string(&path) else {
    panic!("read {}", path.display());
  };
  let Ok(manifest) = serde_json::from_str(&source) else {
    panic!("parse {}", path.display());
  };
  manifest
}

#[expect(clippy::panic, reason = "unreadable corpus files fail the gate")]
fn tree_digest(root: &Path) -> String {
  let mut stack = vec![root.to_path_buf()];
  let mut files = Vec::new();
  while let Some(dir) = stack.pop() {
    let Ok(entries) = fs::read_dir(&dir) else {
      continue;
    };
    for entry in entries.filter_map(Result::ok) {
      let path = entry.path();
      let name = entry.file_name();
      let name = name.to_string_lossy();
      if name == "node_modules" || name == "target" || name == ".DS_Store" {
        continue;
      }
      let Ok(file_type) = entry.file_type() else {
        continue;
      };
      if file_type.is_dir() {
        stack.push(path);
      } else {
        files.push(path);
      }
    }
  }
  files.sort();
  let mut lines = Vec::with_capacity(files.len());
  for path in files {
    let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
    let Ok(bytes) = fs::read(&path) else {
      panic!("read {}", path.display());
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    lines.push(format!("{relative}\t{}", hex_digest(&hasher.finalize())));
  }
  let mut hasher = Sha256::new();
  hasher.update(lines.join("\n").as_bytes());
  hex_digest(&hasher.finalize())
}

fn hex_digest(bytes: &[u8]) -> String {
  let mut output = String::with_capacity(bytes.len().saturating_mul(2));
  for byte in bytes {
    if write!(&mut output, "{byte:02x}").is_err() {
      break;
    }
  }
  output
}

#[expect(clippy::panic, reason = "session setup failures fail the gate")]
fn open_session(root: PathBuf, cache_dir: PathBuf, no_cache: bool) -> ProjectSession {
  match ProjectSession::open(SessionOptions {
    root,
    config_path: None,
    cache_dir: Some(cache_dir),
    no_cache,
    threads: Some(1),
  }) {
    Ok(session) => session,
    Err(error) => panic!("session open: {error}"),
  }
}

#[expect(clippy::panic, reason = "analyze failures fail the gate")]
fn finding_keys(session: &ProjectSession) -> BTreeSet<(String, String)> {
  let snapshot = match session.analyze() {
    Ok(snapshot) => snapshot,
    Err(error) => panic!("analyze: {error}"),
  };
  let root = session.workspace_root();
  let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
  snapshot
    .summary
    .diagnostics
    .iter()
    .map(|diagnostic| {
      let absolute = diagnostic.file.canonicalize().unwrap_or_else(|_| diagnostic.file.clone());
      let file = absolute
        .strip_prefix(&root)
        .unwrap_or(absolute.as_path())
        .to_string_lossy()
        .replace('\\', "/");
      (diagnostic.rule_id.clone(), file)
    })
    .collect()
}

#[expect(clippy::panic, reason = "analyze failures fail the gate")]
fn diagnostic_ids(session: &ProjectSession) -> Vec<String> {
  let snapshot = match session.analyze() {
    Ok(snapshot) => snapshot,
    Err(error) => panic!("analyze: {error}"),
  };
  snapshot
    .summary
    .diagnostics
    .iter()
    .map(|diagnostic| report_diagnostic_id(diagnostic, &snapshot.analyzed_files))
    .collect()
}

#[test]
fn quality_corpus_tree_digests_match_manifest() {
  let manifest = load_manifest();
  assert_eq!(manifest.schema_version, 1);
  assert!(!manifest.projects.is_empty(), "manifest must list projects");
  for project in &manifest.projects {
    let root = workspace_root().join(&project.path);
    assert!(root.is_dir(), "missing corpus project {}", project.path);
    let digest = tree_digest(&root);
    assert_eq!(
      digest, project.tree_digest,
      "tree_digest drift for {}; run `just quality-digest` and update manifest.json if the change is intentional",
      project.id
    );
  }
}

#[test]
#[expect(clippy::panic, reason = "precision fixture failures fail the gate")]
fn precision_expectations_match_scan() {
  let manifest = load_manifest();
  let precision_dir = workspace_root().join("fixtures/quality/precision");
  let Ok(entries) = fs::read_dir(&precision_dir) else {
    panic!("missing {}", precision_dir.display());
  };
  let mut files = entries
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
    .collect::<Vec<_>>();
  files.sort();
  assert!(!files.is_empty(), "precision expectations must exist");

  for path in files {
    let Ok(source) = fs::read_to_string(&path) else {
      panic!("read {}", path.display());
    };
    let Ok(expectation) = serde_json::from_str::<PrecisionFile>(&source) else {
      panic!("parse {}", path.display());
    };
    let Some(project) =
      manifest.projects.iter().find(|candidate| candidate.id == expectation.project)
    else {
      panic!("precision file {} references unknown project", path.display());
    };
    assert!(
      project.roles.iter().any(|role| role == "precision"),
      "project {} must list the precision role",
      project.id
    );

    let mut expected = BTreeSet::new();
    let mut forbidden = BTreeSet::new();
    for finding in &expectation.findings {
      let key = (finding.rule_id.clone(), finding.file.clone());
      match finding.classification.as_str() {
        "true_positive" | "known_limitation" => {
          expected.insert(key);
        }
        "false_positive" => {
          forbidden.insert(key);
        }
        other => panic!("unknown classification `{other}` in {}", path.display()),
      }
    }

    let cache = std::env::temp_dir().join(format!(
      "vue-vet-qg-precision-{}-{}",
      expectation.project,
      std::process::id()
    ));
    let _ignored = fs::remove_dir_all(&cache);
    let session = open_session(workspace_root().join(&project.path), cache.clone(), true);
    let actual = finding_keys(&session);
    let _ignored = fs::remove_dir_all(&cache);

    assert_eq!(
      actual,
      expected,
      "precision mismatch for {} ({})",
      expectation.project,
      path.display()
    );
    for key in &forbidden {
      assert!(
        !actual.contains(key),
        "false_positive {}/{} must not appear for {}",
        key.0,
        key.1,
        expectation.project
      );
    }
  }
}

#[test]
fn cold_and_warm_scans_share_diagnostic_ids() {
  let root = workspace_root().join("fixtures/projects/nuxt-graph");
  let cache = std::env::temp_dir().join(format!("vue-vet-qg-cache-{}", std::process::id()));
  let _ignored = fs::remove_dir_all(&cache);

  let cold = open_session(root.clone(), cache.clone(), false);
  let cold_ids = diagnostic_ids(&cold);
  assert!(!cold_ids.is_empty(), "nuxt-graph must emit findings");

  let warm = open_session(root, cache.clone(), false);
  let warm_ids = diagnostic_ids(&warm);
  assert_eq!(cold_ids, warm_ids, "warm cache must preserve diagnostic identity");
  let _ignored = fs::remove_dir_all(&cache);
}

#[test]
#[ignore = "run via `just quality-digest`"]
#[expect(clippy::print_stdout, reason = "digest printer is a maintainer CLI surface")]
fn digest_printer() {
  let manifest = load_manifest();
  for project in &manifest.projects {
    let root = workspace_root().join(&project.path);
    println!("{}: {}", project.id, tree_digest(&root));
  }
}

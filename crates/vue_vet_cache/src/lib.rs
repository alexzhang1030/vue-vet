//! Content-addressed scan cache, baselines, and git-diff filtering.
//!
//! Persists normalized [`vue_vet_core::ScanSummary`] and
//! [`vue_vet_project::ProjectGraph`] only — never Vize or Oxc AST. Owns
//! `CacheStore`, `Baseline`, and `filter_diff`; does not run analysis.
//! Authoritative versions: [`CACHE_FORMAT_VERSION`], [`RULESET_VERSION`].

use std::{
  collections::{BTreeMap, BTreeSet},
  fmt::Write,
  fs,
  path::{Path, PathBuf},
  process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use vue_vet_core::{Diagnostic, FileId, REACTIVITY_GRAPH_VERSION, ScanSummary};
use vue_vet_project::{CONVENTIONS_VERSION, OXC_RESOLVER_VERSION, ProjectGraph};

pub const CACHE_FORMAT_VERSION: u32 = 5;
pub const BASELINE_FORMAT_VERSION: u32 = 1;
/// Bump when built-in rule set or seed-aware analysis behavior changes.
///
/// v6: watch*Effect self-assign is not a loop (Vue 3.5 coalesces one run);
/// prefer-watch suppresses self-write sources; computed self-write is impure.
/// v5: conditional-dep premise withdrawn; after-await registrars deprecated
/// except defineExpose; absence rules require complete follow coverage.
pub const RULESET_VERSION: u32 = 6;

/// Workspace pin for `vize_croquis` / `vize_atelier_core`.
///
/// Must match `fixtures/quality/compat-matrix.json` and Cargo.toml / Cargo.lock
/// (`just compat-matrix`). Hashed into [`content_key`] via
/// [`AnalysisStackIdentity::current`].
pub const CACHE_VIZE_CROQUIS_VERSION: &str = "0.387.0";
/// Workspace pin for `oxc_parser` / `oxc_semantic`.
///
/// Must match the compat matrix and Cargo.toml / Cargo.lock
/// (`just compat-matrix`).
pub const CACHE_OXC_PARSER_VERSION: &str = "0.142.0";

/// Analysis-stack fields hashed into [`content_key`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisStackIdentity {
  pub vize_croquis: &'static str,
  pub oxc_parser: &'static str,
  pub oxc_resolver: &'static str,
}

impl AnalysisStackIdentity {
  /// Constants actually passed to [`content_key`].
  #[must_use]
  pub const fn current() -> Self {
    Self {
      vize_croquis: CACHE_VIZE_CROQUIS_VERSION,
      oxc_parser: CACHE_OXC_PARSER_VERSION,
      oxc_resolver: OXC_RESOLVER_VERSION,
    }
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CachePayload {
  pub summary: ScanSummary,
  pub graph: ProjectGraph,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheLookup {
  Hit(Box<CachePayload>),
  Miss,
  RecoveredCorruption,
}

#[derive(Debug, Error)]
pub enum CacheError {
  #[error("cache I/O failed for {path}: {message}")]
  Io { path: PathBuf, message: String },
  #[error("cache serialization failed: {0}")]
  Serialize(String),
}

#[derive(Deserialize, Serialize)]
struct CacheEnvelope {
  version: u32,
  payload: CachePayload,
}

pub struct CacheStore {
  root: PathBuf,
}

impl CacheStore {
  #[must_use]
  pub const fn new(root: PathBuf) -> Self {
    Self { root }
  }

  #[must_use]
  pub fn entry_path(&self, key: &str) -> PathBuf {
    self.root.join(format!("v{CACHE_FORMAT_VERSION}")).join(format!("{key}.json"))
  }

  #[must_use]
  pub fn load(&self, key: &str) -> CacheLookup {
    let path = self.entry_path(key);
    let Ok(bytes) = fs::read(&path) else {
      return CacheLookup::Miss;
    };
    match serde_json::from_slice::<CacheEnvelope>(&bytes) {
      Ok(entry) if entry.version == CACHE_FORMAT_VERSION => {
        CacheLookup::Hit(Box::new(entry.payload))
      }
      Ok(_) | Err(_) => {
        let _ignored = fs::remove_file(path);
        CacheLookup::RecoveredCorruption
      }
    }
  }

  /// Atomically store one normalized scan result.
  ///
  /// # Errors
  ///
  /// Returns a path-oriented I/O error or deterministic serialization error.
  pub fn store(&self, key: &str, payload: &CachePayload) -> Result<(), CacheError> {
    let path = self.entry_path(key);
    let Some(parent) = path.parent() else {
      return io_error(&path, "cache entry has no parent directory");
    };
    fs::create_dir_all(parent)
      .map_err(|error| CacheError::Io { path: parent.to_path_buf(), message: error.to_string() })?;
    let bytes = serde_json::to_vec(&CacheEnvelope {
      version: CACHE_FORMAT_VERSION,
      payload: payload.clone(),
    })
    .map_err(|error| CacheError::Serialize(error.to_string()))?;
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)
      .map_err(|error| CacheError::Io { path: temporary.clone(), message: error.to_string() })?;
    fs::rename(&temporary, &path)
      .map_err(|error| CacheError::Io { path, message: error.to_string() })
  }
}

#[must_use]
pub fn default_cache_dir() -> PathBuf {
  std::env::var_os("XDG_CACHE_HOME").map_or_else(
    || std::env::temp_dir().join("vue_vet_cache"),
    |directory| PathBuf::from(directory).join("vue-vet"),
  )
}

#[must_use]
pub fn content_key<T: AsRef<[u8]>>(files: &[(String, T)], config: &[u8]) -> String {
  content_key_with_identity(files, config, AnalysisStackIdentity::current())
}

#[must_use]
pub fn content_key_with_identity<T: AsRef<[u8]>>(
  files: &[(String, T)],
  config: &[u8],
  identity: AnalysisStackIdentity,
) -> String {
  let mut ordered = files.iter().collect::<Vec<_>>();
  ordered.sort_by(|left, right| left.0.cmp(&right.0));
  let mut hasher = Sha256::new();
  hash_field(&mut hasher, b"cache-format", &CACHE_FORMAT_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"tool-version", env!("CARGO_PKG_VERSION").as_bytes());
  hash_field(&mut hasher, b"vize-version", identity.vize_croquis.as_bytes());
  hash_field(&mut hasher, b"oxc-version", identity.oxc_parser.as_bytes());
  hash_field(&mut hasher, b"oxc-resolver-version", identity.oxc_resolver.as_bytes());
  hash_field(&mut hasher, b"conventions-version", &CONVENTIONS_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"ruleset-version", &RULESET_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"reactivity-graph-version", &REACTIVITY_GRAPH_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"config", config);
  for (path, content) in ordered {
    hash_field(&mut hasher, path.as_bytes(), content.as_ref());
  }
  hex_digest(&hasher.finalize())
}

fn hash_field(hasher: &mut Sha256, name: &[u8], value: &[u8]) {
  hasher.update(name.len().to_le_bytes());
  hasher.update(name);
  hasher.update(value.len().to_le_bytes());
  hasher.update(value);
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Baseline {
  pub version: u32,
  pub fingerprints: BTreeSet<String>,
}

impl Baseline {
  #[must_use]
  pub fn from_summary(summary: &ScanSummary) -> Self {
    Self {
      version: BASELINE_FORMAT_VERSION,
      fingerprints: summary.diagnostics.iter().map(diagnostic_fingerprint).collect(),
    }
  }

  #[must_use]
  pub fn filter(&self, mut summary: ScanSummary) -> ScanSummary {
    summary
      .diagnostics
      .retain(|diagnostic| !self.fingerprints.contains(&diagnostic_fingerprint(diagnostic)));
    summary.finish()
  }

  /// Read a versioned baseline file.
  ///
  /// # Errors
  ///
  /// Returns an I/O, JSON, or unsupported-version error.
  pub fn read(path: &Path) -> Result<Self, BaselineError> {
    let bytes = fs::read(path).map_err(|error| BaselineError::Io {
      path: path.to_path_buf(),
      message: error.to_string(),
    })?;
    let baseline = serde_json::from_slice::<Self>(&bytes)
      .map_err(|error| BaselineError::Invalid(error.to_string()))?;
    if baseline.version != BASELINE_FORMAT_VERSION {
      return Err(BaselineError::UnsupportedVersion(baseline.version));
    }
    Ok(baseline)
  }

  /// Atomically write a versioned baseline file.
  ///
  /// # Errors
  ///
  /// Returns an I/O or JSON serialization error.
  pub fn write(&self, path: &Path) -> Result<(), BaselineError> {
    let bytes =
      serde_json::to_vec_pretty(self).map_err(|error| BaselineError::Invalid(error.to_string()))?;
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
      fs::create_dir_all(parent).map_err(|error| BaselineError::Io {
        path: parent.to_path_buf(),
        message: error.to_string(),
      })?;
    }
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&temporary, bytes)
      .map_err(|error| BaselineError::Io { path: temporary.clone(), message: error.to_string() })?;
    if path.exists() {
      fs::remove_file(path).map_err(|error| BaselineError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
      })?;
    }
    fs::rename(&temporary, path)
      .map_err(|error| BaselineError::Io { path: path.to_path_buf(), message: error.to_string() })
  }
}

#[derive(Debug, Error)]
pub enum BaselineError {
  #[error("baseline I/O failed for {path}: {message}")]
  Io { path: PathBuf, message: String },
  #[error("invalid baseline: {0}")]
  Invalid(String),
  #[error("unsupported baseline version {0}")]
  UnsupportedVersion(u32),
}

#[must_use]
pub fn diagnostic_fingerprint(diagnostic: &Diagnostic) -> String {
  let mut hasher = Sha256::new();
  hash_field(&mut hasher, b"fingerprint-version", &BASELINE_FORMAT_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"rule", diagnostic.rule_id.as_bytes());
  hash_field(&mut hasher, b"file", diagnostic.file.as_str().as_bytes());
  hash_field(&mut hasher, b"offset", &diagnostic.span.offset.to_le_bytes());
  hash_field(&mut hasher, b"message", diagnostic.message.as_bytes());
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChangedLines {
  pub files: BTreeMap<String, BTreeSet<usize>>,
}

impl ChangedLines {
  #[must_use]
  pub fn contains(&self, file: &FileId, line: usize) -> bool {
    self.files.get(file.as_str()).is_some_and(|lines| lines.is_empty() || lines.contains(&line))
  }
}

/// Read changed paths and added line ranges using argument-safe Git commands.
///
/// # Errors
///
/// Returns a Git execution or diff parsing error.
pub fn read_git_diff(root: &Path, reference: &str) -> Result<ChangedLines, DiffError> {
  let names = Command::new("git")
    .current_dir(root)
    .args(["diff", "--name-only", "-z", reference, "--"])
    .output()
    .map_err(|error| DiffError::Git(error.to_string()))?;
  if !names.status.success() {
    return Err(DiffError::Git(String::from_utf8_lossy(&names.stderr).into_owned()));
  }
  let mut changed = ChangedLines::default();
  for path in names.stdout.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
    changed.files.entry(String::from_utf8_lossy(path).replace('\\', "/")).or_default();
  }
  let patch = Command::new("git")
    .current_dir(root)
    .args(["diff", "--unified=0", "--no-color", "--no-ext-diff", reference, "--"])
    .output()
    .map_err(|error| DiffError::Git(error.to_string()))?;
  if !patch.status.success() {
    return Err(DiffError::Git(String::from_utf8_lossy(&patch.stderr).into_owned()));
  }
  parse_patch(&String::from_utf8_lossy(&patch.stdout), &mut changed)?;
  Ok(changed)
}

#[derive(Debug, Error)]
pub enum DiffError {
  #[error("git diff failed: {0}")]
  Git(String),
  #[error("invalid git diff hunk `{0}`")]
  InvalidHunk(String),
}

fn parse_patch(diff: &str, changed: &mut ChangedLines) -> Result<(), DiffError> {
  let mut current = None::<String>;
  for line in diff.lines() {
    if let Some(path) = line.strip_prefix("+++ b/") {
      current = Some(path.into());
    } else if line.starts_with("@@") {
      let Some(path) = &current else {
        continue;
      };
      let Some(added) = line.split_whitespace().find(|part| part.starts_with('+')) else {
        return Err(DiffError::InvalidHunk(line.into()));
      };
      let range = added.trim_start_matches('+');
      let (start, count) = range.split_once(',').unwrap_or((range, "1"));
      let start = start.parse::<usize>().map_err(|_| DiffError::InvalidHunk(line.into()))?;
      let count = count.parse::<usize>().map_err(|_| DiffError::InvalidHunk(line.into()))?;
      let lines = changed.files.entry(path.clone()).or_default();
      lines.extend(start..start.saturating_add(count));
    }
  }
  Ok(())
}

#[must_use]
pub fn filter_diff(mut summary: ScanSummary, changed: &ChangedLines) -> ScanSummary {
  summary.diagnostics.retain(|diagnostic| {
    diagnostic.category == "project" || changed.contains(&diagnostic.file, diagnostic.span.line)
  });
  summary.finish()
}

fn io_error<T>(path: &Path, message: &str) -> Result<T, CacheError> {
  Err(CacheError::Io { path: path.to_path_buf(), message: message.into() })
}

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_core::{Confidence, Severity, SourceSpan};

  fn diagnostic(rule: &str, file: &str, line: usize, category: &str) -> Diagnostic {
    Diagnostic {
      rule_id: rule.into(),
      category: category.into(),
      severity: Severity::Warning,
      confidence: Some(Confidence::High),
      documentation: None,
      message: "finding".into(),
      help: None,
      file: file.into(),
      span: SourceSpan { offset: line, length: 1, line, column: 1 },
      edits: Vec::new(),
      recommendation: None,
    }
  }

  #[test]
  fn cache_keys_are_order_independent_and_invalidate_inputs() {
    let first = vec![("b.vue".into(), b"b".to_vec()), ("a.vue".into(), b"a".to_vec())];
    let second = vec![("a.vue".into(), b"a".to_vec()), ("b.vue".into(), b"b".to_vec())];
    assert_eq!(content_key(&first, b"config"), content_key(&second, b"config"));
    assert_ne!(content_key(&first, b"config"), content_key(&second, b"changed"));
  }

  #[test]
  fn content_key_changes_when_analysis_stack_identity_changes() {
    let files = vec![("a.vue".into(), b"a".as_slice())];
    let config = b"cfg";
    let current = AnalysisStackIdentity::current();
    assert_eq!(current.vize_croquis, CACHE_VIZE_CROQUIS_VERSION);
    assert_eq!(current.oxc_parser, CACHE_OXC_PARSER_VERSION);
    assert_eq!(current.oxc_resolver, OXC_RESOLVER_VERSION);
    let base = content_key(&files, config);
    assert_eq!(base, content_key_with_identity(&files, config, current));
    let mut vize = current;
    vize.vize_croquis = "0.0.0-test";
    let mut oxc = current;
    oxc.oxc_parser = "0.0.0-test";
    assert_ne!(base, content_key_with_identity(&files, config, vize));
    assert_ne!(base, content_key_with_identity(&files, config, oxc));
    assert_ne!(
      content_key_with_identity(&files, config, vize),
      content_key_with_identity(&files, config, oxc)
    );
  }

  #[test]
  fn baselines_hide_only_exact_fingerprints() {
    let existing = diagnostic("rule/a", "src/App.vue", 1, "local");
    let added = diagnostic("rule/a", "src/App.vue", 2, "local");
    let baseline = Baseline::from_summary(&ScanSummary {
      files_scanned: 1,
      diagnostics: vec![existing.clone()],
      score: 97,
    });
    let filtered = baseline.filter(ScanSummary {
      files_scanned: 1,
      diagnostics: vec![existing, added.clone()],
      score: 0,
    });
    assert_eq!(filtered.diagnostics, [added]);
  }

  #[test]
  fn diff_filter_retains_changed_lines_and_all_project_findings() {
    let mut changed = ChangedLines::default();
    changed.files.insert("src/App.vue".into(), BTreeSet::from([4]));
    let kept_local = diagnostic("rule/local", "src/App.vue", 4, "local");
    let distant_project = diagnostic("rule/project", "src/Other.vue", 8, "project");
    let hidden = diagnostic("rule/old", "src/App.vue", 2, "local");
    let filtered = filter_diff(
      ScanSummary {
        files_scanned: 2,
        diagnostics: vec![hidden, distant_project.clone(), kept_local.clone()],
        score: 0,
      },
      &changed,
    );
    assert_eq!(filtered.diagnostics, [kept_local, distant_project]);
  }

  #[test]
  fn corrupt_cache_recovers_as_a_miss() {
    let root = std::env::temp_dir().join(format!("vue_vet_cache-test-{}", std::process::id()));
    let store = CacheStore::new(root.clone());
    let path = store.entry_path("broken");
    assert!(path.parent().is_some(), "cache path must have a parent");
    if let Some(parent) = path.parent() {
      assert!(fs::create_dir_all(parent).is_ok(), "test cache directory must be writable");
    }
    assert!(fs::write(&path, b"not json").is_ok(), "corrupt fixture must be writable");
    assert_eq!(store.load("broken"), CacheLookup::RecoveredCorruption);
    let _ignored = fs::remove_dir_all(root);
  }
}

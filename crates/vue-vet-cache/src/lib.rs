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
use vue_vet_core::{Diagnostic, ScanSummary};
use vue_vet_project::{CONVENTIONS_VERSION, ProjectGraph};

pub const CACHE_FORMAT_VERSION: u32 = 3;
pub const BASELINE_FORMAT_VERSION: u32 = 1;
pub const RULESET_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CachePayload {
  pub summary: ScanSummary,
  pub graph: ProjectGraph,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CacheLookup {
  Hit(CachePayload),
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
      Ok(entry) if entry.version == CACHE_FORMAT_VERSION => CacheLookup::Hit(entry.payload),
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
    || std::env::temp_dir().join("vue-vet-cache"),
    |directory| PathBuf::from(directory).join("vue-vet"),
  )
}

#[must_use]
pub fn content_key(files: &[(String, Vec<u8>)], config: &[u8]) -> String {
  let mut ordered = files.iter().collect::<Vec<_>>();
  ordered.sort_by(|left, right| left.0.cmp(&right.0));
  let mut hasher = Sha256::new();
  hash_field(&mut hasher, b"cache-format", &CACHE_FORMAT_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"tool-version", env!("CARGO_PKG_VERSION").as_bytes());
  hash_field(&mut hasher, b"vize-version", b"0.291.0");
  hash_field(&mut hasher, b"oxc-version", b"0.127.0");
  hash_field(&mut hasher, b"conventions-version", &CONVENTIONS_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"ruleset-version", &RULESET_VERSION.to_le_bytes());
  hash_field(&mut hasher, b"config", config);
  for (path, content) in ordered {
    hash_field(&mut hasher, path.as_bytes(), content);
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
  hash_field(&mut hasher, b"file", diagnostic.file.to_string_lossy().replace('\\', "/").as_bytes());
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
  pub fn contains(&self, file: &Path, line: usize) -> bool {
    let path = file.to_string_lossy().replace('\\', "/");
    self.files.iter().any(|(changed, lines)| {
      (path == *changed
        || path.ends_with(&format!("/{changed}"))
        || changed.ends_with(&format!("/{path}")))
        && (lines.is_empty() || lines.contains(&line))
    })
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
      let (start, count) = range.split_once(',').map_or((range, "1"), |parts| parts);
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
    let root = std::env::temp_dir().join(format!("vue-vet-cache-test-{}", std::process::id()));
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

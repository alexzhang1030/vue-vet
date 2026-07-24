use std::{
  collections::BTreeSet,
  fs,
  io::Write,
  path::{Path, PathBuf},
};

use atomic_write_file::AtomicWriteFile;
use thiserror::Error;
use vue_vet_core::{EditApplicability, EditPlan, EditPlanError, TextEdit};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FixMode {
  DryRun,
  Apply,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FixOutcome {
  edit_count: usize,
  file_count: usize,
  changed: bool,
}

impl FixOutcome {
  #[must_use]
  pub const fn edit_count(self) -> usize {
    self.edit_count
  }

  #[must_use]
  pub const fn file_count(self) -> usize {
    self.file_count
  }

  #[must_use]
  pub const fn changed(self) -> bool {
    self.changed
  }
}

#[derive(Debug, Error)]
pub enum FixError {
  #[error(transparent)]
  Plan(#[from] EditPlanError),
  #[error("safe fix I/O failed for {path}: {message}")]
  Io { path: PathBuf, message: String },
  #[error("safe fix target escapes the scan root: {path}")]
  OutsideRoot { path: PathBuf },
  #[error("this safe-fix slice supports one file at a time, but the plan targets {count} files")]
  MultipleFiles { count: usize },
  #[error(
    "edit from `{rule_id}` is outside {path}: byte {offset} with length {length}, file length {source_len}"
  )]
  OutOfBounds { path: PathBuf, rule_id: String, offset: usize, length: usize, source_len: usize },
  #[error("edit from `{rule_id}` splits a UTF-8 code point in {path} at byte {offset}")]
  InvalidUtf8Boundary { path: PathBuf, rule_id: String, offset: usize },
}

/// Validate and preview or atomically apply the active safe edits for one file.
///
/// # Errors
///
/// Returns a deterministic planning error, an invalid source-range error, a
/// scan-root containment error, or an I/O error. No destination file is
/// changed unless the complete plan validates and the atomic commit succeeds.
pub fn execute_safe_edits(
  root: &Path,
  edits: Vec<TextEdit>,
  mode: FixMode,
) -> Result<FixOutcome, FixError> {
  let safe_edits = edits
    .into_iter()
    .filter(|edit| edit.applicability == EditApplicability::Safe)
    .collect::<Vec<_>>();
  if safe_edits.is_empty() {
    return Ok(FixOutcome::default());
  }

  let scope = canonical_scope(root)?;
  let mut resolved_edits = Vec::with_capacity(safe_edits.len());
  for mut edit in safe_edits {
    let target = if edit.file.is_absolute() {
      canonicalize(&edit.file)?
    } else if let Some(exact_file) = &scope.exact_file {
      exact_file.clone()
    } else {
      canonicalize(&scope.boundary.join(&edit.file))?
    };
    if !target.starts_with(&scope.boundary)
      || scope.exact_file.as_ref().is_some_and(|file| file != &target)
    {
      return Err(FixError::OutsideRoot { path: target });
    }
    edit.file = target;
    resolved_edits.push(edit);
  }

  let plan = EditPlan::new(resolved_edits)?;
  let files = plan.edits().iter().map(|edit| edit.file.clone()).collect::<BTreeSet<_>>();
  if files.len() > 1 {
    return Err(FixError::MultipleFiles { count: files.len() });
  }
  let Some(path) = files.first() else {
    return Ok(FixOutcome::default());
  };
  let source = fs::read_to_string(path)
    .map_err(|error| FixError::Io { path: path.clone(), message: error.to_string() })?;
  let updated = apply_plan(path, &source, &plan)?;
  let changed = updated != source;

  if mode == FixMode::Apply && changed {
    let mut destination = AtomicWriteFile::open(path)
      .map_err(|error| FixError::Io { path: path.clone(), message: error.to_string() })?;
    destination
      .write_all(updated.as_bytes())
      .map_err(|error| FixError::Io { path: path.clone(), message: error.to_string() })?;
    destination
      .commit()
      .map_err(|error| FixError::Io { path: path.clone(), message: error.to_string() })?;
  }

  Ok(FixOutcome { edit_count: plan.edits().len(), file_count: files.len(), changed })
}

struct FixScope {
  boundary: PathBuf,
  exact_file: Option<PathBuf>,
}

fn canonical_scope(root: &Path) -> Result<FixScope, FixError> {
  if root.is_dir() {
    return Ok(FixScope { boundary: canonicalize(root)?, exact_file: None });
  }
  let exact_file = canonicalize(root)?;
  let boundary = exact_file
    .parent()
    .filter(|parent| !parent.as_os_str().is_empty())
    .unwrap_or_else(|| Path::new("."))
    .to_path_buf();
  Ok(FixScope { boundary, exact_file: Some(exact_file) })
}

fn canonicalize(path: &Path) -> Result<PathBuf, FixError> {
  fs::canonicalize(path)
    .map_err(|error| FixError::Io { path: path.to_path_buf(), message: error.to_string() })
}

fn apply_plan(path: &Path, source: &str, plan: &EditPlan) -> Result<String, FixError> {
  let mut updated = source.to_owned();
  for edit in plan.edits().iter().rev() {
    let Some(end) = edit.range.end() else {
      return Err(EditPlanError::RangeOverflow { edit: Box::new(edit.clone()) }.into());
    };
    if end > source.len() {
      return Err(FixError::OutOfBounds {
        path: path.to_path_buf(),
        rule_id: edit.rule_id.clone(),
        offset: edit.range.offset,
        length: edit.range.length,
        source_len: source.len(),
      });
    }
    for offset in [edit.range.offset, end] {
      if !source.is_char_boundary(offset) {
        return Err(FixError::InvalidUtf8Boundary {
          path: path.to_path_buf(),
          rule_id: edit.rule_id.clone(),
          offset,
        });
      }
    }
    updated.replace_range(edit.range.offset..end, &edit.replacement);
  }
  Ok(updated)
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use vue_vet_core::ByteRange;

  use super::*;

  static NEXT_TEMP_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

  struct TempDirectory {
    path: PathBuf,
  }

  impl TempDirectory {
    #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
    fn new() -> Self {
      let sequence = NEXT_TEMP_DIRECTORY.fetch_add(1, Ordering::Relaxed);
      let path =
        std::env::temp_dir().join(format!("vue-vet-fixes-{}-{sequence}", std::process::id()));
      let _ignored = fs::remove_dir_all(&path);
      if let Err(error) = fs::create_dir_all(&path) {
        panic!("failed to create temporary directory {}: {error}", path.display());
      }
      Self { path }
    }

    #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
    fn write(&self, name: &str, source: &str) -> PathBuf {
      let path = self.path.join(name);
      if let Err(error) = fs::write(&path, source) {
        panic!("failed to write temporary file {}: {error}", path.display());
      }
      path
    }
  }

  impl Drop for TempDirectory {
    fn drop(&mut self) {
      let _ignored = fs::remove_dir_all(&self.path);
    }
  }

  fn safe_edit(file: PathBuf, offset: usize, length: usize) -> TextEdit {
    TextEdit {
      file,
      range: ByteRange { offset, length },
      replacement: String::new(),
      applicability: EditApplicability::Safe,
      rule_id: "vue-vet/test/fix".into(),
    }
  }

  #[test]
  fn a_single_file_scan_rejects_an_edit_to_a_sibling() {
    let directory = TempDirectory::new();
    let scanned = directory.write("Scanned.vue", "<template />\n");
    let sibling = directory.write("Sibling.vue", "<template autofocus />\n");
    let result =
      execute_safe_edits(&scanned, vec![safe_edit(sibling.clone(), 10, 9)], FixMode::DryRun);
    let canonical_sibling = fs::canonicalize(&sibling);

    assert!(
      matches!(
        (&result, &canonical_sibling),
        (Err(FixError::OutsideRoot { path }), Ok(expected)) if path == expected
      ),
      "a rule must not expand a file-scoped scan to a sibling: {result:?}; canonical sibling: {canonical_sibling:?}"
    );
  }

  #[test]
  fn conflicting_edits_leave_the_original_file_untouched() {
    let directory = TempDirectory::new();
    let source = "0123456789";
    let path = directory.write("App.vue", source);
    let result = execute_safe_edits(
      &path,
      vec![safe_edit(path.clone(), 1, 4), safe_edit(path.clone(), 3, 3)],
      FixMode::Apply,
    );
    let unchanged = fs::read_to_string(&path);

    assert!(
      matches!(result, Err(FixError::Plan(EditPlanError::Conflict { .. }))),
      "overlapping edits must fail during planning: {result:?}"
    );
    assert_eq!(
      unchanged.as_deref().ok(),
      Some(source),
      "planning failure must happen before any write"
    );
  }

  #[test]
  fn out_of_bounds_edits_leave_the_original_file_untouched() {
    let directory = TempDirectory::new();
    let source = "short";
    let path = directory.write("App.vue", source);
    let result =
      execute_safe_edits(&path, vec![safe_edit(path.clone(), source.len(), 1)], FixMode::Apply);
    let unchanged = fs::read_to_string(&path);

    assert!(
      matches!(result, Err(FixError::OutOfBounds { source_len, .. }) if source_len == source.len()),
      "a range beyond the source must fail validation: {result:?}"
    );
    assert_eq!(
      unchanged.as_deref().ok(),
      Some(source),
      "range validation must happen before any write"
    );
  }

  #[test]
  fn edits_cannot_split_a_utf8_code_point() {
    let directory = TempDirectory::new();
    let source = "éx";
    let path = directory.write("App.vue", source);
    let result = execute_safe_edits(&path, vec![safe_edit(path.clone(), 1, 0)], FixMode::Apply);
    let unchanged = fs::read_to_string(&path);

    assert!(
      matches!(result, Err(FixError::InvalidUtf8Boundary { offset: 1, .. })),
      "byte ranges must align with UTF-8 boundaries: {result:?}"
    );
    assert_eq!(
      unchanged.as_deref().ok(),
      Some(source),
      "UTF-8 validation must happen before any write"
    );
  }

  #[test]
  fn multi_file_plans_are_rejected_before_the_first_write() {
    let directory = TempDirectory::new();
    let source = "autofocus";
    let first = directory.write("First.vue", source);
    let second = directory.write("Second.vue", source);
    let result = execute_safe_edits(
      &directory.path,
      vec![safe_edit(first.clone(), 0, source.len()), safe_edit(second.clone(), 0, source.len())],
      FixMode::Apply,
    );
    let first_source = fs::read_to_string(&first);
    let second_source = fs::read_to_string(&second);

    assert!(
      matches!(result, Err(FixError::MultipleFiles { count: 2 })),
      "the first slice must fail closed rather than partially applying files: {result:?}"
    );
    assert_eq!(first_source.as_deref().ok(), Some(source), "the first file must remain unchanged");
    assert_eq!(
      second_source.as_deref().ok(),
      Some(source),
      "the second file must remain unchanged"
    );
  }

  #[test]
  fn unsafe_edits_are_ignored_even_in_apply_mode() {
    let directory = TempDirectory::new();
    let source = "dangerous";
    let path = directory.write("App.vue", source);
    let mut edit = safe_edit(path.clone(), 0, source.len());
    edit.applicability = EditApplicability::Unsafe;
    let result = execute_safe_edits(&path, vec![edit], FixMode::Apply);
    let unchanged = fs::read_to_string(&path);

    assert!(
      matches!(result, std::result::Result::Ok(outcome) if outcome == FixOutcome::default()),
      "unsafe edits must not enter a safe plan"
    );
    assert_eq!(
      unchanged.as_deref().ok(),
      Some(source),
      "safe mode must never write an unsafe edit"
    );
  }

  #[test]
  fn multiple_non_overlapping_edits_apply_against_the_original_source() {
    let directory = TempDirectory::new();
    let path = directory.write("App.vue", "0123456789");
    let result = execute_safe_edits(
      &path,
      vec![safe_edit(path.clone(), 1, 2), safe_edit(path.clone(), 6, 2)],
      FixMode::Apply,
    );
    let rewritten = fs::read_to_string(&path);

    assert!(
      matches!(result, std::result::Result::Ok(outcome) if outcome.edit_count() == 2),
      "both edits must commit as one validated file replacement: {result:?}"
    );
    assert_eq!(
      rewritten.as_deref().ok(),
      Some("034589"),
      "later source ranges must be applied before earlier ranges"
    );
  }
}

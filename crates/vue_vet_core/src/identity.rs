use std::{
  fmt,
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Physical root that defines the namespace for [`FileId`] values.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct WorkspaceRoot(PathBuf);

impl WorkspaceRoot {
  #[must_use]
  pub fn new(path: impl Into<PathBuf>) -> Self {
    Self(path.into())
  }

  #[must_use]
  pub fn as_path(&self) -> &Path {
    &self.0
  }

  #[must_use]
  pub fn into_path_buf(self) -> PathBuf {
    self.0
  }
}

impl AsRef<Path> for WorkspaceRoot {
  fn as_ref(&self) -> &Path {
    self.as_path()
  }
}

/// Physical filesystem location used only at discovery and I/O boundaries.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PhysicalPath(PathBuf);

impl PhysicalPath {
  #[must_use]
  pub fn new(path: impl Into<PathBuf>) -> Self {
    Self(path.into())
  }

  #[must_use]
  pub fn as_path(&self) -> &Path {
    &self.0
  }

  #[must_use]
  pub fn display(&self) -> impl fmt::Display + '_ {
    self.0.display()
  }
}

impl AsRef<Path> for PhysicalPath {
  fn as_ref(&self) -> &Path {
    self.as_path()
  }
}

/// Stable workspace-relative identity for one analyzed source file.
///
/// `FileId` is a logical identity, not an I/O path. Discovery is responsible
/// for making it relative to the workspace root; this type normalizes path
/// separators and redundant `.` segments so every downstream surface compares
/// exact identities instead of path suffixes.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct FileId(String);

impl FileId {
  #[must_use]
  pub fn new(path: impl AsRef<Path>) -> Self {
    Self::from(path.as_ref().to_string_lossy().as_ref())
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  #[must_use]
  pub fn as_path(&self) -> &Path {
    Path::new(&self.0)
  }

  #[must_use]
  pub fn to_path_buf(&self) -> PathBuf {
    PathBuf::from(&self.0)
  }

  #[must_use]
  pub fn display(&self) -> impl fmt::Display + '_ {
    self.0.as_str()
  }

  #[must_use]
  pub fn is_absolute(&self) -> bool {
    self.as_path().is_absolute() || has_windows_drive_prefix(&self.0)
  }
}

impl fmt::Display for FileId {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.0)
  }
}

impl AsRef<Path> for FileId {
  fn as_ref(&self) -> &Path {
    self.as_path()
  }
}

impl From<&Path> for FileId {
  fn from(path: &Path) -> Self {
    Self::new(path)
  }
}

impl From<PathBuf> for FileId {
  fn from(path: PathBuf) -> Self {
    Self::new(path)
  }
}

impl From<&str> for FileId {
  fn from(path: &str) -> Self {
    Self(normalize_file_id(path))
  }
}

impl From<String> for FileId {
  fn from(path: String) -> Self {
    Self::from(path.as_str())
  }
}

fn normalize_file_id(path: &str) -> String {
  let replaced = path.replace('\\', "/");
  let mut prefix = "";
  let mut remainder = replaced.as_str();
  if let Some(stripped) = remainder.strip_prefix('/') {
    prefix = "/";
    remainder = stripped;
  }

  let mut segments = Vec::new();
  for segment in remainder.split('/') {
    match segment {
      "" | "." => {}
      ".." if segments.last().is_some_and(|last| *last != "..") => {
        segments.pop();
      }
      _ => segments.push(segment),
    }
  }

  let normalized = segments.join("/");
  if prefix.is_empty() {
    normalized
  } else if normalized.is_empty() {
    prefix.into()
  } else {
    format!("{prefix}{normalized}")
  }
}

fn has_windows_drive_prefix(path: &str) -> bool {
  let bytes = path.as_bytes();
  bytes.get(1) == Some(&b':') && bytes.first().is_some_and(u8::is_ascii_alphabetic)
}

#[cfg(test)]
mod tests {
  use super::FileId;

  #[test]
  fn normalizes_separator_and_dot_segments() {
    assert_eq!(FileId::from(r"apps\admin\.\src\App.vue").as_str(), "apps/admin/src/App.vue");
    assert_eq!(FileId::from("apps/admin/../shared/App.vue").as_str(), "apps/shared/App.vue");
  }

  #[test]
  fn retains_absolute_marker_only_for_boundary_detection() {
    assert!(FileId::from("/repo/src/App.vue").is_absolute());
    assert!(FileId::from(r"C:\repo\src\App.vue").is_absolute());
    assert!(!FileId::from("src/App.vue").is_absolute());
  }
}

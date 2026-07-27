use std::{
  collections::BTreeMap,
  ffi::OsStr,
  fs,
  path::{Path, PathBuf},
  sync::Arc,
};

use ignore::{DirEntry, WalkBuilder};
use vue_vet_config::Config;
use vue_vet_core::{FileId, PhysicalPath};
use vue_vet_project::resolver_config_inputs;

use crate::{SessionError, package_index::PackageIndex, scan_directory};

#[derive(Clone, Debug)]
pub enum SourceKind {
  Vue,
  Script { language: String },
}

#[derive(Clone, Debug)]
pub struct SourceInput {
  pub physical_path: PhysicalPath,
  pub file_id: FileId,
  pub source: Arc<str>,
  pub kind: SourceKind,
}

/// One immutable filesystem/overlay view shared by cache lookup and analysis.
#[derive(Clone, Debug)]
pub struct WorkspaceInputSnapshot {
  pub boundary: PathBuf,
  pub sources: Vec<SourceInput>,
  pub package_index: PackageIndex,
  pub cache_inputs: Vec<(String, Vec<u8>)>,
  pub analyzed_source_files: Vec<FileId>,
}

impl WorkspaceInputSnapshot {
  pub fn discover(
    root: &Path,
    config: &Config,
    overlays: &BTreeMap<PathBuf, String>,
  ) -> Result<Self, SessionError> {
    if !root.exists() {
      return Err(SessionError::message(format!("path does not exist: {}", root.display())));
    }
    let filter = config.path_filter().map_err(|error| SessionError::message(error.to_string()))?;
    let boundary = scan_directory(root).to_path_buf();
    let mut sources = Vec::new();
    let mut package_index = PackageIndex::default();
    let mut cache_inputs = Vec::new();

    for entry in project_walk(root) {
      let entry = entry.map_err(|error| SessionError::message(error.to_string()))?;
      let path = entry.path();
      if !path.is_file() {
        continue;
      }
      let file_id = FileId::from(logical_path(root, path));
      if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        let bytes = read_bytes(path)?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
          package_index.insert(path, source);
        }
        cache_inputs.push((file_id.as_str().to_owned(), bytes));
        continue;
      }

      let kind = match path.extension().and_then(|extension| extension.to_str()) {
        Some("vue") if filter.matches(file_id.as_path()) => Some(SourceKind::Vue),
        Some(language @ ("js" | "jsx" | "ts" | "tsx")) => {
          Some(SourceKind::Script { language: language.to_owned() })
        }
        _ => None,
      };
      let cache_source = matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("vue" | "js" | "jsx" | "ts" | "tsx")
      );
      if !cache_source {
        continue;
      }
      let bytes = overlay_source(path, overlays)
        .map_or_else(|| read_bytes(path), |source| Ok(source.as_bytes().to_vec()))?;
      cache_inputs.push((file_id.as_str().to_owned(), bytes.clone()));
      if let Some(kind) = kind {
        let source = String::from_utf8(bytes).map_err(|error| {
          SessionError::message(format!("{} is not valid UTF-8: {error}", path.display()))
        })?;
        sources.push(SourceInput {
          physical_path: PhysicalPath::new(path),
          file_id,
          source: Arc::from(source),
          kind,
        });
      }
    }

    if root.is_file() {
      let package_path = boundary.join("package.json");
      if package_path.is_file() {
        let bytes = read_bytes(&package_path)?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
          package_index.insert(&package_path, source);
        }
        cache_inputs.push(("package.json".into(), bytes));
      }
    }

    for relative in resolver_config_inputs(&boundary) {
      let path = boundary.join(&relative);
      if path.is_file() && !cache_inputs.iter().any(|(existing, _)| existing == &relative) {
        cache_inputs.push((relative, read_bytes(&path)?));
      }
    }

    sources.sort_by(|left, right| left.file_id.cmp(&right.file_id));
    cache_inputs.sort_by(|left, right| left.0.cmp(&right.0));
    cache_inputs.dedup_by(|left, right| left.0 == right.0);
    let analyzed_source_files = sources.iter().map(|source| source.file_id.clone()).collect();
    Ok(Self { boundary, sources, package_index, cache_inputs, analyzed_source_files })
  }
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, SessionError> {
  fs::read(path)
    .map_err(|error| SessionError::message(format!("failed to read {}: {error}", path.display())))
}

fn project_walk(root: &Path) -> ignore::Walk {
  WalkBuilder::new(root)
    .standard_filters(true)
    .filter_entry(|entry| !is_node_modules_entry(entry))
    .build()
}

fn is_node_modules_entry(entry: &DirEntry) -> bool {
  entry.file_name() == OsStr::new("node_modules")
}

fn overlay_source<'a>(path: &Path, overlays: &'a BTreeMap<PathBuf, String>) -> Option<&'a str> {
  if let Some(source) = overlays.get(path) {
    return Some(source.as_str());
  }
  let needle = path.to_string_lossy().replace('\\', "/");
  overlays.iter().find_map(|(overlay_path, source)| {
    (overlay_path.to_string_lossy().replace('\\', "/") == needle).then_some(source.as_str())
  })
}

fn logical_path<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
  if root.is_file() {
    path.file_name().map_or(path, |name| Path::new(name))
  } else {
    path.strip_prefix(root).unwrap_or(path)
  }
}

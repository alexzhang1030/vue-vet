use std::{
  collections::{BTreeMap, BTreeSet},
  ffi::OsStr,
  fs,
  path::{Path, PathBuf},
  sync::Arc,
};

use ignore::{DirEntry, WalkBuilder};
use vue_vet_config::Config;
use vue_vet_core::{FileId, PhysicalPath};
use vue_vet_project::{ProjectContext, project_context_from_inputs, resolver_config_inputs};

use crate::{SessionError, package_index::PackageIndex, scan_directory};

#[derive(Clone, Debug, Eq, PartialEq)]
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
  pub cache_inputs: Vec<(String, Arc<[u8]>)>,
  pub analyzed_source_files: Vec<FileId>,
  pub project_context: ProjectContext,
  package_sources: BTreeMap<PathBuf, Arc<str>>,
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
    let mut package_sources = BTreeMap::new();
    let mut cache_inputs = Vec::new();

    for entry in project_walk(root) {
      let entry = entry.map_err(|error| SessionError::message(error.to_string()))?;
      let path = entry.path();
      if !path.is_file() {
        continue;
      }
      let file_id = FileId::from(logical_path(root, path));
      if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        let bytes = overlay_source(path, overlays)
          .map_or_else(|| read_bytes(path), |source| Ok(Arc::from(source.as_bytes())))?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
          package_index.insert(path, source);
          package_sources.insert(path.to_path_buf(), Arc::from(source));
        }
        cache_inputs.push((file_id.as_str().to_owned(), bytes));
        continue;
      }

      let extension = path.extension().and_then(|extension| extension.to_str());
      let kind = source_kind(&file_id, extension, filter.matches(file_id.as_path()));
      let cache_source = matches!(extension, Some("vue" | "js" | "jsx" | "ts" | "tsx"));
      if !cache_source {
        continue;
      }
      let bytes = overlay_source(path, overlays)
        .map_or_else(|| read_bytes(path), |source| Ok(Arc::from(source.as_bytes())))?;
      cache_inputs.push((file_id.as_str().to_owned(), Arc::clone(&bytes)));
      if let Some(kind) = kind {
        let source = String::from_utf8(bytes.as_ref().to_vec()).map_err(|error| {
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
          package_sources.insert(package_path.clone(), Arc::from(source));
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
    let project_context = project_context_from_inputs(
      &boundary,
      &analyzed_source_files,
      cache_inputs.iter().map(|(path, bytes)| (path.as_str(), bytes.as_ref())),
      1,
    );
    Ok(Self {
      boundary,
      sources,
      package_index,
      cache_inputs,
      analyzed_source_files,
      project_context,
      package_sources,
    })
  }

  /// Apply only changed overlay/disk paths to an existing snapshot.
  ///
  /// `Some(source)` installs an in-memory overlay. `None` removes the overlay
  /// and refreshes that exact path from disk (or removes it when deleted).
  pub fn apply_changes(
    &mut self,
    root: &Path,
    config: &Config,
    changes: &BTreeMap<PathBuf, Option<String>>,
  ) -> Result<BTreeSet<FileId>, SessionError> {
    let filter = config.path_filter().map_err(|error| SessionError::message(error.to_string()))?;
    let mut affected_files = BTreeSet::new();
    let mut context_dirty = false;
    for (requested_path, overlay) in changes {
      let path = absolute_change_path(requested_path, &self.boundary);
      if !path.starts_with(&self.boundary) {
        return Err(SessionError::message(format!(
          "changed path escapes workspace: {}",
          requested_path.display()
        )));
      }
      let file_id = FileId::from(logical_path(root, &path));
      let was_source = self.sources.iter().any(|source| source.file_id == file_id);
      let bytes = match overlay {
        Some(source) => Some(Arc::<[u8]>::from(source.as_bytes())),
        None if path.is_file() => Some(read_bytes(&path)?),
        None => None,
      };

      if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        match bytes.as_ref().and_then(|bytes| std::str::from_utf8(bytes).ok()) {
          Some(source) => {
            self.package_sources.insert(path.clone(), Arc::from(source));
          }
          None => {
            self.package_sources.remove(&path);
          }
        }
        self.rebuild_package_index();
        self.update_cache_input(file_id.as_str(), bytes);
        affected_files.extend(self.sources.iter().map(|source| source.file_id.clone()));
        context_dirty = true;
        continue;
      }

      let extension = path.extension().and_then(|extension| extension.to_str());
      let cache_source = matches!(extension, Some("vue" | "js" | "jsx" | "ts" | "tsx"))
        || self.cache_inputs.iter().any(|(existing, _)| existing == file_id.as_str())
        || resolver_config_inputs(&self.boundary).iter().any(|input| input == file_id.as_str());
      if cache_source {
        self.update_cache_input(file_id.as_str(), bytes.clone());
      }

      let resolver_input = is_project_context_input(&file_id);
      let kind = source_kind(&file_id, extension, filter.matches(file_id.as_path()));
      if let (Some(kind), Some(bytes)) = (kind, bytes) {
        let source = String::from_utf8(bytes.as_ref().to_vec()).map_err(|error| {
          SessionError::message(format!("{} is not valid UTF-8: {error}", path.display()))
        })?;
        self.upsert_source(SourceInput {
          physical_path: PhysicalPath::new(&path),
          file_id: file_id.clone(),
          source: Arc::from(source),
          kind,
        });
      } else {
        self.sources.retain(|source| source.file_id != file_id);
      }
      let is_source = self.sources.iter().any(|source| source.file_id == file_id);
      if was_source != is_source || resolver_input {
        context_dirty = true;
      }
      affected_files.insert(file_id);
    }
    self.sources.sort_by(|left, right| left.file_id.cmp(&right.file_id));
    self.analyzed_source_files = self.sources.iter().map(|source| source.file_id.clone()).collect();
    self.cache_inputs.sort_by(|left, right| left.0.cmp(&right.0));
    if context_dirty {
      let revision = self.project_context.revision.saturating_add(1);
      self.project_context = project_context_from_inputs(
        &self.boundary,
        &self.analyzed_source_files,
        self.cache_inputs.iter().map(|(path, bytes)| (path.as_str(), bytes.as_ref())),
        revision,
      );
    }
    Ok(affected_files)
  }

  fn upsert_source(&mut self, input: SourceInput) {
    if let Some(existing) = self.sources.iter_mut().find(|source| source.file_id == input.file_id) {
      *existing = input;
    } else {
      self.sources.push(input);
    }
  }

  fn update_cache_input(&mut self, file: &str, bytes: Option<Arc<[u8]>>) {
    self.cache_inputs.retain(|(existing, _)| existing != file);
    if let Some(bytes) = bytes {
      self.cache_inputs.push((file.to_owned(), bytes));
    }
  }

  fn rebuild_package_index(&mut self) {
    self.package_index = PackageIndex::default();
    for (path, source) in &self.package_sources {
      self.package_index.insert(path, source);
    }
  }
}

fn read_bytes(path: &Path) -> Result<Arc<[u8]>, SessionError> {
  fs::read(path)
    .map(Arc::from)
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

fn absolute_change_path(path: &Path, boundary: &Path) -> PathBuf {
  if path.is_absolute() { path.to_path_buf() } else { boundary.join(path) }
}

fn is_project_context_input(file: &FileId) -> bool {
  let path = file.as_str();
  let name = file.as_path().file_name().and_then(|name| name.to_str()).unwrap_or(path);
  matches!(
    path,
    "package.json"
      | "package-lock.json"
      | "pnpm-lock.yaml"
      | "yarn.lock"
      | "bun.lock"
      | "bun.lockb"
      | "tsconfig.json"
      | "tsconfig.app.json"
      | "tsconfig.node.json"
      | ".nuxt/tsconfig.json"
      | ".nuxt/components.d.ts"
      | ".nuxt/types/components.d.ts"
  ) || (name.starts_with("tsconfig")
    && Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")))
}

fn is_generated_resolver_input(file: &FileId) -> bool {
  matches!(file.as_str(), ".nuxt/components.d.ts" | ".nuxt/types/components.d.ts")
}

fn source_kind(file: &FileId, extension: Option<&str>, include_vue: bool) -> Option<SourceKind> {
  if is_generated_resolver_input(file) {
    return None;
  }
  match extension {
    Some("vue") if include_vue => Some(SourceKind::Vue),
    Some(language @ ("js" | "jsx" | "ts" | "tsx")) => {
      Some(SourceKind::Script { language: language.to_owned() })
    }
    _ => None,
  }
}

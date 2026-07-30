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
use vue_vet_project::{
  ContextChangeKind, ProjectContext, context_change_kind_for, project_context_from_inputs,
  resolver_config_inputs,
};

use crate::{SessionError, discover_workspace_boundary, package_index::PackageIndex};

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
    // File scans walk up to the nearest package.json so Vite/Nuxt maps resolve.
    let boundary = discover_workspace_boundary(root);
    let mut sources = Vec::new();
    let mut package_index = PackageIndex::default();
    let mut package_sources = BTreeMap::new();
    let mut cache_inputs = Vec::new();
    let mut seen_file_ids = BTreeSet::new();

    for entry in project_walk(root) {
      let entry = entry.map_err(|error| SessionError::message(error.to_string()))?;
      let path = entry.path();
      if !path.is_file() {
        continue;
      }
      let file_id = file_id_for_physical(root, &boundary, path);
      if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
        let bytes = overlay_source(path, overlays)
          .map_or_else(|| read_bytes(path), |source| Ok(Arc::from(source.as_bytes())))?;
        if let Ok(source) = std::str::from_utf8(&bytes) {
          package_index.insert(path, source);
          package_sources.insert(path.to_path_buf(), Arc::from(source));
        }
        cache_inputs.push((file_id.as_str().to_owned(), bytes));
        seen_file_ids.insert(file_id);
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
          file_id: file_id.clone(),
          source: Arc::from(source),
          kind,
        });
      }
      seen_file_ids.insert(file_id);
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
        seen_file_ids.insert(FileId::from("package.json"));
      }
    }

    for relative in resolver_config_inputs(&boundary) {
      let path = boundary.join(&relative);
      if path.is_file() && !cache_inputs.iter().any(|(existing, _)| existing == &relative) {
        cache_inputs.push((relative.clone(), read_bytes(&path)?));
        seen_file_ids.insert(FileId::from(relative.as_str()));
      }
    }

    merge_overlay_only_sources(
      root,
      &boundary,
      overlays,
      &filter,
      &mut OverlayMergeState {
        sources: &mut sources,
        package_index: &mut package_index,
        package_sources: &mut package_sources,
        cache_inputs: &mut cache_inputs,
        seen_file_ids: &mut seen_file_ids,
      },
    );

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
  /// Strong exception safety: on `Err`, `self` is left unchanged.
  ///
  /// `Some(source)` installs an in-memory overlay. `None` removes the overlay
  /// and refreshes that exact path from disk (or removes it when deleted).
  /// Strong exception safety: on `Err`, `self` is left unchanged.
  /// Mutate this snapshot in place. On `Err`, contents may be partially updated;
  /// callers that need strong exception safety must clone first (session does this
  /// via `Arc::make_mut` on a forked inputs Arc, then drops the fork on failure).
  pub(crate) fn apply_changes_in_place(
    &mut self,
    root: &Path,
    config: &Config,
    changes: &BTreeMap<PathBuf, Option<String>>,
  ) -> Result<BTreeSet<FileId>, SessionError> {
    let filter = config.path_filter().map_err(|error| SessionError::message(error.to_string()))?;
    let mut affected_files = BTreeSet::new();
    let mut epochs = self.project_context.epochs;
    let mut context_dirty = false;
    for (requested_path, overlay) in changes {
      let path = absolute_change_path(requested_path, &self.boundary);
      if !path.starts_with(&self.boundary) {
        return Err(SessionError::message(format!(
          "changed path escapes workspace: {}",
          requested_path.display()
        )));
      }
      let file_id = file_id_for_physical(root, &self.boundary, &path);
      let was_source = self.sources.iter().any(|source| source.file_id == file_id);
      let bytes = match overlay {
        Some(source) => Some(Arc::<[u8]>::from(source.as_bytes())),
        None if path.is_file() => Some(read_bytes(&path)?),
        None => None,
      };

      if let Some(kind) = context_change_kind_for(file_id.as_str()) {
        epochs.bump(kind);
        context_dirty = true;
      }

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
        affected_files.insert(file_id);
        continue;
      }

      let extension = path.extension().and_then(|extension| extension.to_str());
      let cache_source = matches!(extension, Some("vue" | "js" | "jsx" | "ts" | "tsx"))
        || self.cache_inputs.iter().any(|(existing, _)| existing == file_id.as_str())
        || resolver_config_inputs(&self.boundary).iter().any(|input| input == file_id.as_str());
      if cache_source {
        self.update_cache_input(file_id.as_str(), bytes.clone());
      }

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
      if was_source != is_source {
        epochs.bump(ContextChangeKind::SourceMembership);
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
      self.project_context.epochs = epochs;
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

/// Workspace-relative [`FileId`] for a physical path under `root` / `boundary`.
///
/// Directory scans strip `root`. File scans strip `boundary` (package root when
/// discovered) so importer paths and diagnostic ids stay package-relative.
#[must_use]
pub fn file_id_for_physical(root: &Path, boundary: &Path, path: &Path) -> FileId {
  FileId::from(logical_path(root, boundary, path))
}

struct OverlayMergeState<'a> {
  sources: &'a mut Vec<SourceInput>,
  package_index: &'a mut PackageIndex,
  package_sources: &'a mut BTreeMap<PathBuf, Arc<str>>,
  cache_inputs: &'a mut Vec<(String, Arc<[u8]>)>,
  seen_file_ids: &'a mut BTreeSet<FileId>,
}

fn merge_overlay_only_sources(
  root: &Path,
  boundary: &Path,
  overlays: &BTreeMap<PathBuf, String>,
  filter: &vue_vet_config::PathFilter,
  state: &mut OverlayMergeState<'_>,
) {
  for (overlay_path, source) in overlays {
    let path = absolute_change_path(overlay_path, boundary);
    if !path.starts_with(boundary) {
      continue;
    }
    let file_id = file_id_for_physical(root, boundary, &path);
    if state.seen_file_ids.contains(&file_id) {
      continue;
    }
    if path.file_name().and_then(|name| name.to_str()) == Some("package.json") {
      state.package_index.insert(&path, source);
      state.package_sources.insert(path.clone(), Arc::from(source.as_str()));
      state.cache_inputs.push((file_id.as_str().to_owned(), Arc::from(source.as_bytes())));
      state.seen_file_ids.insert(file_id);
      continue;
    }
    let extension = path.extension().and_then(|extension| extension.to_str());
    let cache_source = matches!(extension, Some("vue" | "js" | "jsx" | "ts" | "tsx"))
      || context_change_kind_for(file_id.as_str()).is_some();
    if !cache_source {
      continue;
    }
    let bytes: Arc<[u8]> = Arc::from(source.as_bytes());
    state.cache_inputs.push((file_id.as_str().to_owned(), Arc::clone(&bytes)));
    if let Some(kind) = source_kind(&file_id, extension, filter.matches(file_id.as_path())) {
      state.sources.push(SourceInput {
        physical_path: PhysicalPath::new(&path),
        file_id: file_id.clone(),
        source: Arc::from(source.as_str()),
        kind,
      });
    }
    state.seen_file_ids.insert(file_id);
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

fn logical_path<'a>(root: &'a Path, boundary: &'a Path, path: &'a Path) -> &'a Path {
  if root.is_file() {
    path.strip_prefix(boundary).unwrap_or_else(|_| path.file_name().map_or(path, Path::new))
  } else {
    path.strip_prefix(root).unwrap_or(path)
  }
}

fn absolute_change_path(path: &Path, boundary: &Path) -> PathBuf {
  if path.is_absolute() { path.to_path_buf() } else { boundary.join(path) }
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

#[cfg(test)]
mod tests {
  use super::*;
  use vue_vet_config::Config;

  #[test]
  #[expect(clippy::panic, reason = "discovery fixture failures must fail the unit test")]
  fn failed_apply_changes_leaves_snapshot_unchanged() {
    let root = std::env::temp_dir().join(format!("vue-vet-apply-atomic-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
    let good = root.join("Good.vue");
    std::fs::write(&good, "<template><main /></template>")
      .unwrap_or_else(|error| panic!("good: {error}"));
    let bad = root.join("bad.ts");
    std::fs::write(&bad, "export const ok = 1;\n").unwrap_or_else(|error| panic!("bad: {error}"));
    let snapshot = WorkspaceInputSnapshot::discover(&root, &Config::default(), &BTreeMap::new())
      .unwrap_or_else(|error| panic!("discover: {error}"));
    let before = snapshot.clone();
    std::fs::write(&bad, [0xff, 0xfe, 0xfd])
      .unwrap_or_else(|error| panic!("invalid bytes: {error}"));
    let changes = BTreeMap::from([
      (good, Some("<template><main v-html=\"html\" /></template>".into())),
      (bad, None),
    ]);
    let mut next = snapshot.clone();
    let Err(error) = next.apply_changes_in_place(&root, &Config::default(), &changes) else {
      panic!("invalid UTF-8 refresh must fail");
    };
    assert!(error.to_string().contains("UTF-8"), "{error}");
    // Failed in-place mutation must not be committed onto the live snapshot.
    assert_eq!(snapshot.sources.len(), before.sources.len());
    assert_eq!(
      snapshot.sources.iter().map(|source| source.file_id.as_str()).collect::<Vec<_>>(),
      before.sources.iter().map(|source| source.file_id.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(snapshot.project_context.revision, before.project_context.revision);
    assert!(
      snapshot
        .sources
        .iter()
        .any(|source| source.file_id.as_str() == "Good.vue" && !source.source.contains("v-html")),
      "failed mutation must not install the earlier overlay in the batch"
    );
    let _ignored = std::fs::remove_dir_all(root);
  }

  #[test]
  #[expect(clippy::panic, reason = "discovery fixture failures must fail the unit test")]
  fn single_file_discover_walks_up_to_package_auto_imports() {
    let root = std::env::temp_dir().join(format!("vue-vet-file-boundary-{}", std::process::id()));
    let nested = root.join("src/pages/deep");
    std::fs::create_dir_all(&nested).unwrap_or_else(|error| panic!("dirs: {error}"));
    std::fs::write(root.join("package.json"), r#"{"name":"app"}"#)
      .unwrap_or_else(|error| panic!("package: {error}"));
    std::fs::write(
      root.join("auto-imports.d.ts"),
      "export {}\ndeclare global {\n  const useTableQuery: typeof import('./src/composables/useTable')['useTableQuery']\n}\n",
    )
    .unwrap_or_else(|error| panic!("auto-imports: {error}"));
    let file = nested.join("index.tsx");
    std::fs::write(&file, "export const ok = 1\n").unwrap_or_else(|error| panic!("file: {error}"));

    let snapshot = WorkspaceInputSnapshot::discover(&file, &Config::default(), &BTreeMap::new())
      .unwrap_or_else(|error| panic!("discover: {error}"));
    let expected_boundary = root.canonicalize().unwrap_or_else(|_| root.clone());
    let actual_boundary =
      snapshot.boundary.canonicalize().unwrap_or_else(|_| snapshot.boundary.clone());
    assert_eq!(
      actual_boundary, expected_boundary,
      "single-file scan must use the package root as boundary"
    );
    assert_eq!(
      snapshot
        .project_context
        .nuxt_import_names
        .get("useTableQuery")
        .map(|t| (t.importer.as_str(), t.specifier.as_str())),
      Some(("auto-imports.d.ts", "./src/composables/useTable")),
      "package-root auto-imports.d.ts must load for nested file scans"
    );
    assert_eq!(
      snapshot.sources.iter().map(|s| s.file_id.as_str()).collect::<Vec<_>>(),
      ["src/pages/deep/index.tsx"],
      "file ids must be package-relative for nested single-file scans"
    );
    let _ignored = std::fs::remove_dir_all(root);
  }

  #[test]
  #[expect(clippy::panic, reason = "discovery fixture failures must fail the unit test")]
  fn discover_includes_overlay_only_unsaved_sources() {
    let root = std::env::temp_dir().join(format!("vue-vet-overlay-only-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("workspace: {error}"));
    std::fs::write(root.join("Existing.vue"), "<template><main /></template>")
      .unwrap_or_else(|error| panic!("existing: {error}"));
    let overlays = BTreeMap::from([(
      root.join("NewComponent.vue"),
      "<template><main v-html=\"html\" /></template>".into(),
    )]);
    let snapshot = WorkspaceInputSnapshot::discover(&root, &Config::default(), &overlays)
      .unwrap_or_else(|error| panic!("discover: {error}"));
    assert!(
      snapshot.sources.iter().any(|source| source.file_id.as_str() == "NewComponent.vue"),
      "overlay-only unsaved files must enter the first discovery snapshot"
    );
    let _ignored = std::fs::remove_dir_all(root);
  }
}

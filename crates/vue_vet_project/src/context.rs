//! `ConventionsLoad` — build [`ProjectContext`] from filesystem or input snapshot.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use vue_vet_core::FileId;

use crate::conventions::{
  NUXT_COMPONENT_DTS_CANDIDATES, NUXT_IMPORTS_DTS_CANDIDATES, NuxtImportTarget, OwnerConfigFacts,
  is_nuxt_config_file, load_nuxt_component_dts_names, load_nuxt_imports_dts_names, parent_dir_key,
  parse_nuxt_components_dts, parse_nuxt_imports_dts, source_nuxt_content_evidence,
  source_owner_config_facts,
};
use crate::resolve::{
  ProjectResolver, Resolution, normalize_project_root, normalized_path, resolver_config_inputs,
};

/// Why a resolver-context epoch advanced — drives typed incremental invalidation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextChangeKind {
  PackageManifest,
  Lockfile,
  TsConfig,
  NuxtDeclarations,
  SourceMembership,
}

/// Independent epochs so debounced / batched mutations cannot drop a prior kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContextEpochs {
  pub package_manifest: u64,
  pub lockfile: u64,
  pub tsconfig: u64,
  pub nuxt_declarations: u64,
  pub source_membership: u64,
}

impl ContextEpochs {
  /// Advance the epoch for `kind`.
  pub const fn bump(&mut self, kind: ContextChangeKind) {
    match kind {
      ContextChangeKind::PackageManifest => {
        self.package_manifest = self.package_manifest.wrapping_add(1);
      }
      ContextChangeKind::Lockfile => {
        self.lockfile = self.lockfile.wrapping_add(1);
      }
      ContextChangeKind::TsConfig => {
        self.tsconfig = self.tsconfig.wrapping_add(1);
      }
      ContextChangeKind::NuxtDeclarations => {
        self.nuxt_declarations = self.nuxt_declarations.wrapping_add(1);
      }
      ContextChangeKind::SourceMembership => {
        self.source_membership = self.source_membership.wrapping_add(1);
      }
    }
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectContext {
  pub revision: u64,
  pub nuxt_component_names: BTreeMap<String, String>,
  /// Bare auto-import name → specifier + declaring dts importer.
  pub nuxt_import_names: BTreeMap<String, NuxtImportTarget>,
  /// Package-relative dirs where `@nuxt/content` is enabled (`""` = project root).
  pub nuxt_content_roots: BTreeSet<String>,
  /// Package.json / `nuxt.config.*` parent dirs that bound Content ownership.
  pub convention_owners: BTreeSet<String>,
  /// Literal `srcDir` per owner, when statically known.
  pub nuxt_src_dirs: BTreeMap<String, String>,
  pub invalidation_inputs: Vec<String>,
  /// Per-kind epochs consumed by long-lived incremental analysis.
  pub epochs: ContextEpochs,
}

impl ProjectContext {
  #[must_use]
  pub fn from_filesystem(root: &Path, known: &BTreeSet<String>) -> Self {
    let root = normalize_project_root(root);
    let loaded = load_nuxt_content_from_fs(&root, known);
    let mut invalidation_inputs = resolver_config_inputs(&root);
    invalidation_inputs.extend(loaded.invalidation_inputs);
    invalidation_inputs.sort();
    invalidation_inputs.dedup();
    Self {
      revision: 0,
      nuxt_component_names: load_nuxt_component_dts_names(&root, known),
      nuxt_import_names: load_nuxt_imports_dts_names(&root),
      nuxt_content_roots: loaded.content_roots,
      convention_owners: loaded.owners,
      nuxt_src_dirs: loaded.src_dirs,
      invalidation_inputs,
      epochs: ContextEpochs::default(),
    }
  }
}

/// Build project context from the already-read workspace input snapshot.
#[must_use]
pub fn project_context_from_inputs<'a>(
  root: &Path,
  known_files: impl IntoIterator<Item = &'a FileId>,
  inputs: impl IntoIterator<Item = (&'a str, &'a [u8])>,
  revision: u64,
) -> ProjectContext {
  let root = normalize_project_root(root);
  let known =
    known_files.into_iter().map(|file| normalized_path(file.as_path())).collect::<BTreeSet<_>>();
  let mut nuxt_component_names = BTreeMap::new();
  let mut nuxt_import_names = BTreeMap::new();
  let mut owner_facts = BTreeMap::<String, OwnerConfigFacts>::new();
  let mut invalidation_inputs = Vec::new();
  let input_map = inputs.into_iter().collect::<BTreeMap<_, _>>();
  let mut candidate_paths = config_candidates_from_known(&known);
  candidate_paths.extend(
    input_map.keys().copied().filter(|path| is_ownership_config_path(path)).map(str::to_owned),
  );
  for (relative, bytes) in &input_map {
    if is_project_invalidation_input(relative) {
      invalidation_inputs.push((*relative).to_owned());
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
      continue;
    };
    if NUXT_COMPONENT_DTS_CANDIDATES.contains(relative) {
      let path = root.join(relative);
      for (name, target) in parse_nuxt_components_dts(&path, source, &root, &known) {
        nuxt_component_names.insert(name, target);
      }
    }
    if NUXT_IMPORTS_DTS_CANDIDATES.contains(relative) {
      for (name, specifier) in parse_nuxt_imports_dts(source) {
        // First dts wins (sorted cache inputs hit `.nuxt/imports.d.ts` before types).
        nuxt_import_names
          .entry(name)
          .or_insert_with(|| NuxtImportTarget { specifier, importer: (*relative).to_owned() });
      }
    }
  }
  for relative in candidate_paths {
    if !is_ownership_config_path(&relative) {
      continue;
    }
    let Some(bytes) = input_map.get(relative.as_str()) else {
      continue;
    };
    let Ok(source) = std::str::from_utf8(bytes) else {
      continue;
    };
    let owner = parent_dir_key(&relative);
    let facts = source_owner_config_facts(&relative, source);
    owner_facts
      .entry(owner)
      .and_modify(|existing| *existing = existing.merge(facts.clone()))
      .or_insert(facts);
  }
  let loaded = finish_owner_facts(&root, owner_facts, Some(&input_map));
  invalidation_inputs.extend(loaded.invalidation_inputs);
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectContext {
    revision,
    nuxt_component_names,
    nuxt_import_names,
    nuxt_content_roots: loaded.content_roots,
    convention_owners: loaded.owners,
    nuxt_src_dirs: loaded.src_dirs,
    invalidation_inputs,
    epochs: ContextEpochs::default(),
  }
}

const CONFIG_FILE_NAMES: &[&str] = &[
  "package.json",
  "nuxt.config.ts",
  "nuxt.config.js",
  "nuxt.config.mjs",
  "nuxt.config.mts",
  "nuxt.config.cjs",
];

fn config_candidates_from_known(known: &BTreeSet<String>) -> BTreeSet<String> {
  let mut candidates = BTreeSet::new();
  for relative in known {
    let name = Path::new(relative).file_name().and_then(|name| name.to_str()).unwrap_or(relative);
    if name == "package.json" || is_nuxt_config_file(name) {
      candidates.insert(relative.clone());
    }
    let mut current = Path::new(relative);
    while let Some(parent) = current.parent() {
      for name in CONFIG_FILE_NAMES {
        let parent_key = normalized_path(parent);
        let candidate =
          if parent_key.is_empty() { (*name).to_owned() } else { format!("{parent_key}/{name}") };
        candidates.insert(candidate);
      }
      if parent.as_os_str().is_empty() {
        break;
      }
      current = parent;
    }
  }
  candidates
}

struct LoadedContentContext {
  content_roots: BTreeSet<String>,
  owners: BTreeSet<String>,
  src_dirs: BTreeMap<String, String>,
  invalidation_inputs: Vec<String>,
}

fn load_nuxt_content_from_fs(root: &Path, known: &BTreeSet<String>) -> LoadedContentContext {
  let mut candidates = config_candidates_from_known(known);
  for input in resolver_config_inputs(root) {
    if is_ownership_config_path(&input) {
      candidates.insert(input);
    }
  }
  let mut owner_facts = BTreeMap::<String, OwnerConfigFacts>::new();
  for relative in candidates {
    if !is_ownership_config_path(&relative) {
      continue;
    }
    let path = root.join(&relative);
    let Ok(source) = std::fs::read_to_string(&path) else {
      continue;
    };
    let owner = parent_dir_key(&relative);
    let facts = source_owner_config_facts(&relative, &source);
    owner_facts
      .entry(owner)
      .and_modify(|existing| *existing = existing.merge(facts.clone()))
      .or_insert(facts);
  }
  finish_owner_facts(root, owner_facts, None)
}

fn finish_owner_facts(
  root: &Path,
  mut owner_facts: BTreeMap<String, OwnerConfigFacts>,
  input_map: Option<&BTreeMap<&str, &[u8]>>,
) -> LoadedContentContext {
  let mut invalidation_inputs = Vec::new();
  apply_layer_extends(root, &mut owner_facts, &mut invalidation_inputs, input_map);
  let mut content_roots = BTreeSet::new();
  let mut owners = BTreeSet::new();
  let mut src_dirs = BTreeMap::new();
  for (owner, facts) in owner_facts {
    owners.insert(owner.clone());
    if let Some(src_dir) = facts.src_dir {
      src_dirs.insert(owner.clone(), src_dir);
    }
    if facts.evidence.enables() {
      content_roots.insert(owner);
    }
  }
  LoadedContentContext { content_roots, owners, src_dirs, invalidation_inputs }
}

fn apply_layer_extends(
  root: &Path,
  owner_facts: &mut BTreeMap<String, OwnerConfigFacts>,
  invalidation_inputs: &mut Vec<String>,
  input_map: Option<&BTreeMap<&str, &[u8]>>,
) {
  let resolver = ProjectResolver::new(root);
  let specs = owner_facts
    .iter()
    .filter(|(_, facts)| !facts.extends.is_empty())
    .map(|(owner, facts)| (owner.clone(), facts.extends.clone()))
    .collect::<Vec<_>>();
  for (owner, extends) in specs {
    let importer = if owner.is_empty() {
      "nuxt.config.ts".to_owned()
    } else {
      format!("{owner}/nuxt.config.ts")
    };
    let mut visited = BTreeSet::new();
    let layer =
      follow_layer_extends(root, &resolver, &importer, &extends, &mut visited, 0, input_map);
    invalidation_inputs.extend(layer.invalidation_inputs);
    if let Some(facts) = owner_facts.get_mut(&owner) {
      facts.evidence = facts.evidence.merge(layer.evidence);
    }
  }
}

struct LayerEvidence {
  evidence: crate::conventions::OwnerContentEvidence,
  invalidation_inputs: Vec<String>,
}

fn follow_layer_extends(
  root: &Path,
  resolver: &ProjectResolver,
  importer: &str,
  extends: &[String],
  visited: &mut BTreeSet<String>,
  depth: u8,
  input_map: Option<&BTreeMap<&str, &[u8]>>,
) -> LayerEvidence {
  let mut combined = LayerEvidence {
    evidence: crate::conventions::OwnerContentEvidence::default(),
    invalidation_inputs: Vec::new(),
  };
  if depth > 8 {
    return combined;
  }
  let importer_absolute = root.join(importer);
  for specifier in extends {
    let Some(absolute) = resolve_layer_path(resolver, &importer_absolute, specifier) else {
      continue;
    };
    let Some(package_dir) = layer_package_dir(&absolute) else {
      continue;
    };
    let key = normalized_path(&package_dir);
    if !visited.insert(key) {
      continue;
    }
    if let Some(relative) = package_dir.strip_prefix(root).ok().map(normalized_path) {
      let package_json = if relative.is_empty() {
        "package.json".to_owned()
      } else {
        format!("{relative}/package.json")
      };
      if let Some(source) = layer_utf8(&package_json, &package_dir.join("package.json"), input_map)
      {
        combined.evidence =
          combined.evidence.merge(source_nuxt_content_evidence(&package_json, &source));
        combined.invalidation_inputs.push(package_json);
      }
    } else if input_map.is_none()
      && let Ok(source) = std::fs::read_to_string(package_dir.join("package.json"))
    {
      combined.evidence =
        combined.evidence.merge(source_nuxt_content_evidence("package.json", &source));
    }
    if let Some((config_relative, source)) = read_layer_nuxt_config(root, &package_dir, input_map) {
      combined.invalidation_inputs.push(config_relative.clone());
      let facts = source_owner_config_facts(&config_relative, &source);
      combined.evidence = combined.evidence.merge(facts.evidence);
      if !facts.extends.is_empty() {
        let nested = follow_layer_extends(
          root,
          resolver,
          &config_relative,
          &facts.extends,
          visited,
          depth.saturating_add(1),
          input_map,
        );
        combined.evidence = combined.evidence.merge(nested.evidence);
        combined.invalidation_inputs.extend(nested.invalidation_inputs);
      }
    }
  }
  combined
}

fn resolve_layer_path(
  resolver: &ProjectResolver,
  importer_absolute: &Path,
  specifier: &str,
) -> Option<PathBuf> {
  let candidates = [
    specifier.to_owned(),
    format!("{specifier}/package.json"),
    format!("{specifier}/nuxt.config"),
    format!("{specifier}/nuxt.config.ts"),
  ];
  for candidate in candidates {
    if let Resolution::External { resolved_path: Some(absolute), .. } =
      resolver.resolve_from_absolute(importer_absolute, &candidate)
    {
      return Some(absolute);
    }
  }
  None
}

fn layer_package_dir(resolved: &Path) -> Option<PathBuf> {
  let name = resolved.file_name().and_then(|name| name.to_str()).unwrap_or("");
  if name == "package.json" || is_nuxt_config_file(name) {
    return resolved.parent().map(Path::to_path_buf);
  }
  if resolved.is_dir() {
    Some(resolved.to_path_buf())
  } else {
    resolved.parent().map(Path::to_path_buf)
  }
}

fn read_layer_nuxt_config(
  root: &Path,
  package_dir: &Path,
  input_map: Option<&BTreeMap<&str, &[u8]>>,
) -> Option<(String, String)> {
  for name in CONFIG_FILE_NAMES {
    if *name == "package.json" {
      continue;
    }
    let absolute = package_dir.join(name);
    let relative = absolute
      .strip_prefix(root)
      .ok()
      .map_or_else(|| normalized_path(absolute.as_path()), normalized_path);
    if let Some(source) = layer_utf8(&relative, &absolute, input_map) {
      return Some((relative, source));
    }
  }
  None
}

fn layer_utf8(
  relative: &str,
  absolute: &Path,
  input_map: Option<&BTreeMap<&str, &[u8]>>,
) -> Option<String> {
  if let Some(inputs) = input_map {
    let bytes = inputs.get(relative)?;
    return std::str::from_utf8(bytes).ok().map(str::to_owned);
  }
  std::fs::read_to_string(absolute).ok()
}

/// Layer config / package paths reachable from already-retained input bytes.
///
/// Nested extends of files that are not yet in `inputs` are not expanded;
/// callers load missing relatives and invoke again.
#[must_use]
pub fn layer_input_relatives<'a>(
  root: &Path,
  inputs: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Vec<String> {
  let root = normalize_project_root(root);
  let input_map = inputs.into_iter().collect::<BTreeMap<_, _>>();
  let resolver = ProjectResolver::new(&root);
  let mut relatives = BTreeSet::new();
  let mut visited = BTreeSet::new();
  for (path, bytes) in &input_map {
    if !is_nuxt_config_file(
      Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path),
    ) {
      continue;
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
      continue;
    };
    let facts = source_owner_config_facts(path, source);
    collect_layer_relatives(
      &root,
      &resolver,
      path,
      &facts.extends,
      &mut visited,
      &mut relatives,
      0,
    );
  }
  relatives.into_iter().collect()
}

fn collect_layer_relatives(
  root: &Path,
  resolver: &ProjectResolver,
  importer: &str,
  extends: &[String],
  visited: &mut BTreeSet<String>,
  relatives: &mut BTreeSet<String>,
  depth: u8,
) {
  if depth > 8 {
    return;
  }
  let importer_absolute = root.join(importer);
  for specifier in extends {
    let Some(absolute) = resolve_layer_path(resolver, &importer_absolute, specifier) else {
      continue;
    };
    let Some(package_dir) = layer_package_dir(&absolute) else {
      continue;
    };
    let Some(relative_dir) = package_dir.strip_prefix(root).ok().map(normalized_path) else {
      continue;
    };
    if !visited.insert(relative_dir.clone()) {
      continue;
    }
    let package_json = if relative_dir.is_empty() {
      "package.json".to_owned()
    } else {
      format!("{relative_dir}/package.json")
    };
    relatives.insert(package_json);
    for name in CONFIG_FILE_NAMES {
      if *name == "package.json" {
        continue;
      }
      let config =
        if relative_dir.is_empty() { (*name).to_owned() } else { format!("{relative_dir}/{name}") };
      relatives.insert(config);
    }
  }
}

fn is_ownership_config_path(path: &str) -> bool {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  name == "package.json" || is_nuxt_config_file(name)
}

fn is_project_invalidation_input(path: &str) -> bool {
  context_change_kind_for(path).is_some()
}

/// Classify a workspace-relative path as a typed resolver-context change.
#[must_use]
pub fn context_change_kind_for(path: &str) -> Option<ContextChangeKind> {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  if name == "package.json" || is_nuxt_config_file(name) {
    return Some(ContextChangeKind::PackageManifest);
  }
  if matches!(path, "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "bun.lock" | "bun.lockb")
  {
    return Some(ContextChangeKind::Lockfile);
  }
  if matches!(
    path,
    ".nuxt/components.d.ts"
      | ".nuxt/types/components.d.ts"
      | ".nuxt/imports.d.ts"
      | ".nuxt/types/imports.d.ts"
      | "auto-imports.d.ts"
      | "src/auto-imports.d.ts"
  ) {
    return Some(ContextChangeKind::NuxtDeclarations);
  }
  if matches!(
    path,
    "tsconfig.json" | "tsconfig.app.json" | "tsconfig.node.json" | ".nuxt/tsconfig.json"
  ) || (name.starts_with("tsconfig")
    && Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("json")))
  {
    return Some(ContextChangeKind::TsConfig);
  }
  None
}

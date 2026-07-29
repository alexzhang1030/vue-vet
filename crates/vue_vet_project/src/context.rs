//! `ConventionsLoad` — build [`ProjectContext`] from filesystem or input snapshot.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
};

use vue_vet_core::FileId;

use crate::conventions::{
  NUXT_COMPONENT_DTS_CANDIDATES, NUXT_IMPORTS_DTS_CANDIDATES, NuxtImportTarget,
  load_nuxt_component_dts_names, load_nuxt_imports_dts_names, parse_nuxt_components_dts,
  parse_nuxt_imports_dts,
};
use crate::resolve::{normalize_project_root, normalized_path, resolver_config_inputs};

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
  pub invalidation_inputs: Vec<String>,
  /// Per-kind epochs consumed by long-lived incremental analysis.
  pub epochs: ContextEpochs,
}

impl ProjectContext {
  #[must_use]
  pub fn from_filesystem(root: &Path, known: &BTreeSet<String>) -> Self {
    let root = normalize_project_root(root);
    Self {
      revision: 0,
      nuxt_component_names: load_nuxt_component_dts_names(&root, known),
      nuxt_import_names: load_nuxt_imports_dts_names(&root),
      invalidation_inputs: resolver_config_inputs(&root),
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
  let mut invalidation_inputs = Vec::new();
  for (relative, bytes) in inputs {
    if is_project_invalidation_input(relative) {
      invalidation_inputs.push(relative.to_owned());
    }
    let Ok(source) = std::str::from_utf8(bytes) else {
      continue;
    };
    if NUXT_COMPONENT_DTS_CANDIDATES.contains(&relative) {
      let path = root.join(relative);
      for (name, target) in parse_nuxt_components_dts(&path, source, &root, &known) {
        nuxt_component_names.insert(name, target);
      }
    }
    if NUXT_IMPORTS_DTS_CANDIDATES.contains(&relative) {
      for (name, specifier) in parse_nuxt_imports_dts(source) {
        // First dts wins (sorted cache inputs hit `.nuxt/imports.d.ts` before types).
        nuxt_import_names
          .entry(name)
          .or_insert_with(|| NuxtImportTarget { specifier, importer: relative.to_owned() });
      }
    }
  }
  invalidation_inputs.sort();
  invalidation_inputs.dedup();
  ProjectContext {
    revision,
    nuxt_component_names,
    nuxt_import_names,
    invalidation_inputs,
    epochs: ContextEpochs::default(),
  }
}

fn is_project_invalidation_input(path: &str) -> bool {
  context_change_kind_for(path).is_some()
}

/// Classify a workspace-relative path as a typed resolver-context change.
#[must_use]
pub fn context_change_kind_for(path: &str) -> Option<ContextChangeKind> {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  if name == "package.json" {
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

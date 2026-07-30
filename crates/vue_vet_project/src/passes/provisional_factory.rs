//! [`ProvisionalFactoryMergePass`] — [`SummaryMerge`](super::EnrichmentStage::SummaryMerge)
//! enrichment for provisional Factory halves and `ComponentFactory` bodies.
//!
//! Invoked at each module completion inside [`ExternalSummaryLoadPass`](super::ExternalSummaryLoadPass)
//! (same traversal; not a hidden side effect).

use std::{
  path::{Path, PathBuf},
  sync::Arc,
};

use vue_vet_reactivity::{
  ModuleSource, ModuleSummary, merge_declaration_implementation_summary,
  prepare_standalone_module_source,
};

use crate::resolve::language_for_path;

/// Skip companion implementation bodies larger than this (bytes). Nuxt/VueUse
/// runtime composables are tiny; multi‑MB bundles like `typescript.js` are not
/// reactivity evidence and must not be parsed.
///
/// 1 MiB fits typical mid-size design-system entry bundles while still rejecting
/// multi‑MB toolchain packages.
pub const EXTERNAL_COMPANION_MAX_BYTES: u64 = 1024 * 1024;

/// Provisional `.d.ts` + size-capped companion body → Factory / `ComponentFactory` evidence.
pub struct ProvisionalFactoryMergePass;

impl ProvisionalFactoryMergePass {
  pub const NAME: &'static str = "provisional_factory_merge";
  pub const STAGE: super::EnrichmentStage = super::EnrichmentStage::SummaryMerge;

  /// When a `.d.ts` summary needs body evidence, load companion `.js` / package
  /// `exports.import` (size-bounded) and merge.
  ///
  /// Quiet under-approx: missing files, oversize bodies, or parse failures return
  /// `None` without inventing seeds.
  #[must_use]
  pub fn run(types_path: &Path, module: &ModuleSource) -> Option<ModuleSource> {
    Self::run_with_limit(types_path, module, EXTERNAL_COMPANION_MAX_BYTES)
  }

  #[must_use]
  pub fn run_with_limit(
    types_path: &Path,
    module: &ModuleSource,
    max_bytes: u64,
  ) -> Option<ModuleSource> {
    let summary = module.module_summary()?;
    let wants_provisional = summary.needs_implementation_merge();
    let wants_component_factory = summary.may_gain_component_factory_from_impl()
      && dts_may_wrap_define_component(module.source.as_ref());
    if !wants_provisional && !wants_component_factory {
      return None;
    }
    let impl_path = companion_implementation_path(types_path)
      .or_else(|| package_exports_import_path(types_path))?;
    let mut impl_summary = load_summary_under_cap(&impl_path, max_bytes)?;
    // Bundled entries often re-export minified chunk locals — one relative hop.
    if wants_component_factory
      && !impl_summary.has_component_factory_local()
      && let Some(enriched) =
        follow_relative_component_factory(&impl_path, &impl_summary, max_bytes)
    {
      impl_summary = enriched;
    }
    if !wants_provisional && !impl_summary.has_component_factory_local() {
      return None;
    }
    let merged = merge_declaration_implementation_summary(summary.as_ref(), impl_summary.as_ref());
    Some(module.clone().with_module_summary(merged))
  }
}

fn dts_may_wrap_define_component(source: &str) -> bool {
  source.contains("defineComponent")
    || (source.contains("SetupContext") && source.contains("RenderFunction"))
}

fn load_summary_under_cap(path: &Path, max_bytes: u64) -> Option<Arc<ModuleSummary>> {
  let metadata = std::fs::metadata(path).ok()?;
  if !metadata.is_file() || metadata.len() > max_bytes {
    return None;
  }
  let source_text = std::fs::read_to_string(path).ok()?;
  let language = language_for_path(path);
  let impl_module =
    prepare_standalone_module_source(path.display().to_string(), source_text, language).ok()?;
  impl_module.module_summary()
}

/// One relative import hop when the entry only re-exports a chunk that defines
/// `ComponentFactory` locals.
fn follow_relative_component_factory(
  entry_path: &Path,
  entry_summary: &ModuleSummary,
  max_bytes: u64,
) -> Option<Arc<ModuleSummary>> {
  let parent = entry_path.parent()?;
  let mut acc: Option<ModuleSummary> = None;
  for specifier in entry_summary.follow_specifiers() {
    if !(specifier.starts_with("./") || specifier.starts_with("../")) {
      continue;
    }
    let candidate = resolve_relative_js(parent, &specifier)?;
    let Some(child) = load_summary_under_cap(&candidate, max_bytes) else {
      continue;
    };
    if !child.has_component_factory_local() {
      continue;
    }
    let base = acc.as_ref().unwrap_or(entry_summary);
    acc = Some(merge_declaration_implementation_summary(base, child.as_ref()));
  }
  acc.filter(ModuleSummary::has_component_factory_local).map(Arc::new)
}

fn resolve_relative_js(parent: &Path, specifier: &str) -> Option<PathBuf> {
  let direct = parent.join(specifier);
  if direct.is_file() {
    return Some(direct);
  }
  for extension in [".js", ".mjs", ".cjs"] {
    let with_ext = if specifier.ends_with(extension) {
      parent.join(specifier)
    } else {
      parent.join(format!("{specifier}{extension}"))
    };
    if with_ext.is_file() {
      return Some(with_ext);
    }
  }
  None
}

#[must_use]
fn companion_implementation_path(types_path: &Path) -> Option<PathBuf> {
  let file_name = types_path.file_name().and_then(|name| name.to_str())?;
  let stem = file_name
    .strip_suffix(".d.ts")
    .or_else(|| file_name.strip_suffix(".d.mts"))
    .or_else(|| file_name.strip_suffix(".d.cts"))?;
  for extension in [".js", ".mjs", ".cjs", ".ts", ".mts", ".cts"] {
    let candidate = types_path.with_file_name(format!("{stem}{extension}"));
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  None
}

/// `package.json` `exports["."].import` (or `module` / `main`) near a types path.
#[must_use]
fn package_exports_import_path(types_path: &Path) -> Option<PathBuf> {
  let mut dir = types_path.parent()?;
  loop {
    let package_json = dir.join("package.json");
    if package_json.is_file() {
      let text = std::fs::read_to_string(&package_json).ok()?;
      let import_rel = package_import_specifier(&text)?;
      let candidate = dir.join(import_rel);
      if candidate.is_file() {
        return Some(candidate);
      }
      return None;
    }
    dir = dir.parent()?;
  }
}

fn package_import_specifier(package_json: &str) -> Option<String> {
  let value: serde_json::Value = serde_json::from_str(package_json).ok()?;
  if let Some(import) = value
    .pointer("/exports/.")
    .and_then(|entry| match entry {
      serde_json::Value::String(path) => Some(path.as_str()),
      serde_json::Value::Object(map) => map.get("import").and_then(serde_json::Value::as_str),
      _ => None,
    })
    .map(str::to_owned)
  {
    return Some(import);
  }
  value
    .get("module")
    .or_else(|| value.get("main"))
    .and_then(serde_json::Value::as_str)
    .map(str::to_owned)
}

#[cfg(test)]
mod tests {
  use super::{
    EXTERNAL_COMPANION_MAX_BYTES, ProvisionalFactoryMergePass, companion_implementation_path,
    package_exports_import_path,
  };
  use vue_vet_core::FileId;
  use vue_vet_reactivity::prepare_standalone_module_source;

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  fn companion_path_prefers_js_beside_dts() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-factory-pass-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("dir: {error}"));
    let dts = dir.join("composables.d.ts");
    let js = dir.join("composables.js");
    std::fs::write(&dts, "export declare const useX: () => { value: string };\n")
      .unwrap_or_else(|error| panic!("dts: {error}"));
    std::fs::write(&js, "export const useX = () => ({ value: 'a' })\n")
      .unwrap_or_else(|error| panic!("js: {error}"));
    assert_eq!(companion_implementation_path(&dts).as_deref(), Some(js.as_path()));
    drop(std::fs::remove_dir_all(&dir));
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  fn package_exports_import_resolves_from_nested_types() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-factory-pkg-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
    ));
    drop(std::fs::remove_dir_all(&dir));
    let types = dir.join("dist/types/utils");
    std::fs::create_dir_all(&types).unwrap_or_else(|error| panic!("dir: {error}"));
    std::fs::create_dir_all(dir.join("dist")).unwrap_or_else(|error| panic!("dist: {error}"));
    std::fs::write(
      dir.join("package.json"),
      r#"{"name":"pkg","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
    )
    .unwrap_or_else(|error| panic!("pkg: {error}"));
    let js = dir.join("dist/index.js");
    std::fs::write(&js, "export function defineTypedComponent() {}\n")
      .unwrap_or_else(|error| panic!("js: {error}"));
    let dts = types.join("defineTypedComponent.d.ts");
    std::fs::write(&dts, "export declare function defineTypedComponent(): void;\n")
      .unwrap_or_else(|error| panic!("dts: {error}"));
    assert_eq!(package_exports_import_path(&dts).as_deref(), Some(js.as_path()));
    drop(std::fs::remove_dir_all(&dir));
  }

  #[test]
  #[expect(clippy::panic, reason = "test setup failures must fail the unit test")]
  fn size_cap_skips_huge_companion() {
    let dir = std::env::temp_dir().join(format!(
      "vue-vet-factory-cap-{}-{}",
      std::process::id(),
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("dir: {error}"));
    let dts = dir.join("composables.d.ts");
    let js = dir.join("composables.js");
    let dts_source = "export interface Mode { value: string }\n\
export declare const useColorMode: () => Mode;\n";
    let js_source = "export const useColorMode = () => useState('x').value\n";
    std::fs::write(&dts, dts_source).unwrap_or_else(|error| panic!("dts: {error}"));
    std::fs::write(&js, js_source).unwrap_or_else(|error| panic!("js: {error}"));

    let module =
      prepare_standalone_module_source(FileId::from("composables.d.ts"), dts_source, "d.ts")
        .unwrap_or_else(|error| panic!("prepare: {error}"));
    assert!(
      module.module_summary().is_some_and(|summary| summary.needs_implementation_merge()),
      "fixture must be provisional"
    );
    assert!(
      ProvisionalFactoryMergePass::run_with_limit(&dts, &module, 1).is_none(),
      "oversize companion must be skipped"
    );
    assert!(
      (js_source.len() as u64) <= EXTERNAL_COMPANION_MAX_BYTES,
      "fixture body must fit under default companion cap"
    );
    drop(std::fs::remove_dir_all(&dir));
  }
}

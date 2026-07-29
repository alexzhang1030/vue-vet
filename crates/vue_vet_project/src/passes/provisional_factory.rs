//! `ProvisionalFactoryMergePass` — [`SummaryMerge`](super::EnrichmentPhase::SummaryMerge)
//! enrichment for provisional Factory halves.

use std::path::{Path, PathBuf};

use vue_vet_reactivity::{
  ModuleSource, merge_declaration_implementation_summary, prepare_standalone_module_source,
};

use crate::resolve::language_for_path;

/// Skip companion implementation bodies larger than this (bytes). Nuxt/VueUse
/// runtime composables are tiny; multi‑MB bundles like `typescript.js` are not
/// reactivity evidence and must not be parsed.
pub const EXTERNAL_COMPANION_MAX_BYTES: u64 = 512 * 1024;

/// When a `.d.ts` summary has provisional Factory halves, load companion
/// `.js`/`.mjs`/`.ts` body evidence and merge (size-bounded).
///
/// Quiet under-approx: missing files, oversize bodies, or parse failures return
/// `None` without inventing `Factory(Reactive)`.
#[must_use]
pub fn apply_provisional_factory_merge(
  types_path: &Path,
  module: &ModuleSource,
) -> Option<ModuleSource> {
  apply_provisional_factory_merge_with_limit(types_path, module, EXTERNAL_COMPANION_MAX_BYTES)
}

#[must_use]
pub fn apply_provisional_factory_merge_with_limit(
  types_path: &Path,
  module: &ModuleSource,
  max_bytes: u64,
) -> Option<ModuleSource> {
  let summary = module.module_summary()?;
  if !summary.needs_implementation_merge() {
    return None;
  }
  let impl_path = companion_implementation_path(types_path)?;
  let metadata = std::fs::metadata(&impl_path).ok()?;
  if !metadata.is_file() || metadata.len() > max_bytes {
    return None;
  }
  let source_text = std::fs::read_to_string(&impl_path).ok()?;
  let language = language_for_path(&impl_path);
  let impl_module =
    prepare_standalone_module_source(module.id.clone(), source_text, language).ok()?;
  let impl_summary = impl_module.module_summary()?;
  let merged = merge_declaration_implementation_summary((*summary).clone(), impl_summary.as_ref());
  Some(module.clone().with_module_summary(merged))
}

#[must_use]
pub fn companion_implementation_path(types_path: &Path) -> Option<PathBuf> {
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

#[cfg(test)]
mod tests {
  use super::{
    EXTERNAL_COMPANION_MAX_BYTES, apply_provisional_factory_merge_with_limit,
    companion_implementation_path,
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
      apply_provisional_factory_merge_with_limit(&dts, &module, 1).is_none(),
      "oversize companion must be skipped"
    );
    assert!(
      (js_source.len() as u64) <= EXTERNAL_COMPANION_MAX_BYTES,
      "fixture body must fit under default companion cap"
    );
    drop(std::fs::remove_dir_all(&dir));
  }
}

//! [`ExternalSummaryLoadPass`] — [`ExternalSummaryLoad`](super::EnrichmentStage::ExternalSummaryLoad)
//! enrichment: load external package summaries for reactivity seed follow.
//!
//! Per loaded module, [`ProvisionalFactoryMergePass`](super::ProvisionalFactoryMergePass)
//! runs at completion ([`SummaryMerge`](super::EnrichmentStage::SummaryMerge)).

use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  path::{Path, PathBuf},
};

use vue_vet_core::ModuleId;
use vue_vet_reactivity::{ModuleLink, ModuleSource, prepare_standalone_module_source};

use super::{ExternalReactivityRoot, ProvisionalFactoryMergePass};
use crate::resolve::{ProjectResolver, Resolution, language_for_path, prefer_types_declaration};

mod dts;
mod paths;

use dts::enrich_dts_with_relative_type_imports;
use paths::{
  canonicalize_external_path, external_module_id, package_queue_key, relative_types_follow_path,
};

/// Max external package files loaded for reactivity seed follow (under-approx budget).
///
/// Large UI barrels (`export *` over dozens of components) need ~70+ files before a
/// leaf like `Form/utils.d.ts` is reached; 128 was exhausted by vue-query + other
/// priority-2 packages before that leaf loaded in real apps.
const EXTERNAL_REACTIVITY_MAX_FILES: usize = 512;
/// Soft cap per `node_modules` package so one barrel cannot monopolize the budget.
const EXTERNAL_REACTIVITY_MAX_FILES_PER_PACKAGE: usize = 128;
/// Max re-export follow depth from an external entry.
const EXTERNAL_REACTIVITY_MAX_DEPTH: usize = 8;

/// Lower loads first. Budget is finite — prefer packages that commonly back
/// `MethodForward` leaves (`useQuery`) over ambient type-only graphs.
fn external_root_priority(specifier: &str) -> u8 {
  if specifier == "vue"
    || specifier.starts_with("vue/")
    || specifier.starts_with("@vue/")
    || specifier == "@tanstack/vue-query"
    || specifier.starts_with("@tanstack/vue-query/")
  {
    0
  } else if specifier.starts_with("@vueuse/") || specifier.contains("vueuse") {
    1
  } else {
    2
  }
}

/// Load external `.d.ts` / package bodies reached from structural seed roots.
pub struct ExternalSummaryLoadPass;

impl ExternalSummaryLoadPass {
  pub const NAME: &'static str = "external_summary_load";
  pub const STAGE: super::EnrichmentStage = super::EnrichmentStage::ExternalSummaryLoad;

  /// Follow `roots` into external packages; apply `SummaryMerge` per loaded module.
  #[must_use]
  pub fn run(
    root: &Path,
    resolver: &ProjectResolver,
    roots: &[ExternalReactivityRoot],
    on_external_seeds: Option<&dyn Fn(usize)>,
  ) -> (Vec<ModuleSource>, Vec<ModuleLink>) {
    let mut sources = Vec::new();
    let mut links = Vec::new();
    let mut loaded: BTreeMap<PathBuf, ModuleId> = BTreeMap::new();
    let mut loaded_per_package: BTreeMap<String, usize> = BTreeMap::new();

    if !roots.is_empty()
      && let Some(on_external_seeds) = on_external_seeds
    {
      on_external_seeds(roots.len());
    }

    // One queue entry per resolved path; keep every importer→package ModuleLink.
    // Expand **one package at a time** (priority, then importer popularity) so a
    // deep UI barrel can reach `Form/utils.d.ts` before hundreds of ambient
    // package entries consume the global file budget.
    let mut package_queues: BTreeMap<String, VecDeque<(PathBuf, usize)>> = BTreeMap::new();
    let mut package_priority: BTreeMap<String, u8> = BTreeMap::new();
    let mut package_root_count: BTreeMap<String, usize> = BTreeMap::new();
    let mut queued_roots = BTreeSet::new();
    for root_link in roots {
      let path = canonicalize_external_path(&prefer_types_declaration(&root_link.resolved_path));
      let module_id = external_module_id(root, &path);
      links.push(ModuleLink {
        from: root_link.from.clone(),
        specifier: root_link.specifier.clone(),
        to: module_id,
      });
      let package = package_queue_key(&path);
      *package_root_count.entry(package.clone()).or_insert(0) += 1;
      let priority = external_root_priority(&root_link.specifier);
      package_priority
        .entry(package.clone())
        .and_modify(|existing| *existing = (*existing).min(priority))
        .or_insert(priority);
      if queued_roots.insert(path.clone()) {
        package_queues.entry(package).or_default().push_back((path, 0));
      }
    }

    let mut package_order: Vec<String> = package_queues.keys().cloned().collect();
    package_order.sort_by(|left, right| {
      let left_key = (
        package_priority.get(left).copied().unwrap_or(u8::MAX),
        std::cmp::Reverse(package_root_count.get(left).copied().unwrap_or(0)),
        left.as_str(),
      );
      let right_key = (
        package_priority.get(right).copied().unwrap_or(u8::MAX),
        std::cmp::Reverse(package_root_count.get(right).copied().unwrap_or(0)),
        right.as_str(),
      );
      left_key.cmp(&right_key)
    });
    // Worklist so `typeof` bare-package follows discovered mid-traversal still load.
    let mut pending_packages: VecDeque<String> = package_order.into();
    let mut scheduled_packages: BTreeSet<String> = pending_packages.iter().cloned().collect();

    while let Some(package) = pending_packages.pop_front() {
      if sources.len() >= EXTERNAL_REACTIVITY_MAX_FILES {
        break;
      }
      while sources.len() < EXTERNAL_REACTIVITY_MAX_FILES {
        let package_count = loaded_per_package.get(&package).copied().unwrap_or(0);
        if package_count >= EXTERNAL_REACTIVITY_MAX_FILES_PER_PACKAGE {
          break;
        }
        let Some((path, depth)) =
          package_queues.get_mut(&package).and_then(std::collections::VecDeque::pop_front)
        else {
          break;
        };
        if loaded.contains_key(&path) {
          continue;
        }
        let Ok(source_text) = std::fs::read_to_string(&path) else {
          continue;
        };
        let module_id = external_module_id(root, &path);
        let language = language_for_path(&path);
        // `.d.ts` barrels often `import type` the same symbols many relative files
        // re-export. Concatenating those files for same-file lookup can produce
        // duplicate bindings and fail Oxc semantics — fall back to the raw file so
        // re-export follow still loads leaf shapes (e.g. vue-query `useQuery`).
        let Ok(mut module) = (if language == "d.ts" {
          let enriched = enrich_dts_with_relative_type_imports(&path, &source_text);
          prepare_standalone_module_source(module_id.clone(), enriched, language)
            .or_else(|_| prepare_standalone_module_source(module_id.clone(), source_text, language))
        } else {
          prepare_standalone_module_source(module_id.clone(), source_text, language)
        }) else {
          continue;
        };
        // SummaryMerge at ExternalSummaryLoad completion (same traversal).
        if let Some(merged) = ProvisionalFactoryMergePass::run(&path, &module) {
          module = merged;
        }
        let typeof_forward_bares = module
          .module_summary()
          .map(|summary| summary.typeof_forward_sources())
          .unwrap_or_default();
        // `export * from 'other-pkg'` / `export { x } from 'other-pkg'` — the
        // package surface is incomplete until that bare target is loaded (e.g.
        // `@vueuse/core` → `@vueuse/shared` for `useTimeout`).
        let reexport_bare_packages = module
          .module_summary()
          .map(|summary| summary.reexport_bare_package_sources())
          .unwrap_or_default();
        let follow =
          module.module_summary().map(|summary| summary.follow_specifiers()).unwrap_or_default();
        loaded.insert(path.clone(), module_id.clone());
        *loaded_per_package.entry(package.clone()).or_insert(0) += 1;
        sources.push(module);

        if depth >= EXTERNAL_REACTIVITY_MAX_DEPTH {
          continue;
        }
        for specifier in follow {
          let is_relative = specifier.starts_with("./") || specifier.starts_with("../");
          // Bare packages stay quiet unless a `typeof` forward needs that import
          // or this module re-exports from that package.
          if !is_relative
            && !typeof_forward_bares.contains(&specifier)
            && !reexport_bare_packages.contains(&specifier)
          {
            continue;
          }
          let child = match resolver.resolve_from_absolute(&path, &specifier) {
            Resolution::External { resolved_path: Some(target_path), .. }
              if target_path.is_file() =>
            {
              prefer_types_declaration(&target_path)
            }
            // Directory barrels (`export * from './components'`) and types-only
            // chunks (`./queryClient-HASH.js` → `.d.ts`) need a filesystem fallback
            // when bundler resolve returns a directory or Unresolved.
            _ if is_relative => match relative_types_follow_path(&path, &specifier) {
              Some(dts) => dts,
              None => continue,
            },
            _ => continue,
          };
          let child = canonicalize_external_path(&child);
          let child_id = external_module_id(root, &child);
          links.push(ModuleLink { from: module_id.clone(), specifier, to: child_id.clone() });
          if !loaded.contains_key(&child) {
            let child_package = package_queue_key(&child);
            package_queues.entry(child_package.clone()).or_default().push_back((child, depth + 1));
            if scheduled_packages.insert(child_package.clone()) {
              package_priority.entry(child_package.clone()).or_insert(2);
              pending_packages.push_back(child_package);
            }
          }
        }
      }
    }

    // Drop links whose target failed to load (quiet under-approx).
    let loaded_ids = sources.iter().map(|module| module.id.clone()).collect::<BTreeSet<_>>();
    links.retain(|link| loaded_ids.contains(&link.to));
    // Stable order for determinism.
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    links.sort_by(|left, right| {
      (&left.from, &left.specifier, &left.to).cmp(&(&right.from, &right.specifier, &right.to))
    });
    links.dedup();
    (sources, links)
  }
}

#[cfg(test)]
#[expect(
  clippy::unwrap_used,
  clippy::panic,
  clippy::let_underscore_must_use,
  reason = "fixture setup in unit tests"
)]
mod tests;

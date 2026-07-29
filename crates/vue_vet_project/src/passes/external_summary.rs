//! [`ExternalSummaryLoadPass`] — [`ExternalSummaryLoad`](super::EnrichmentStage::ExternalSummaryLoad)
//! enrichment: load external package summaries for reactivity seed follow.
//!
//! Per loaded module, [`ProvisionalFactoryMergePass`](super::ProvisionalFactoryMergePass)
//! runs at completion ([`SummaryMerge`](super::EnrichmentStage::SummaryMerge)).

use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  path::{Path, PathBuf},
};

use vue_vet_core::{FileId, ModuleId};
use vue_vet_reactivity::{ModuleLink, ModuleSource, prepare_standalone_module_source};

use super::{ExternalReactivityRoot, ProvisionalFactoryMergePass};
use crate::resolve::{
  ProjectResolver, Resolution, language_for_path, normalized_path, prefer_types_declaration,
};

/// Max external package files loaded for reactivity seed follow (under-approx budget).
const EXTERNAL_REACTIVITY_MAX_FILES: usize = 64;
/// Max re-export follow depth from an external entry.
const EXTERNAL_REACTIVITY_MAX_DEPTH: usize = 8;

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
    let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();

    if !roots.is_empty()
      && let Some(on_external_seeds) = on_external_seeds
    {
      on_external_seeds(roots.len());
    }

    for root_link in roots {
      let path = prefer_types_declaration(&root_link.resolved_path);
      queue.push_back((path.clone(), 0));
      let module_id = external_module_id(root, &path);
      links.push(ModuleLink {
        from: root_link.from.clone(),
        specifier: root_link.specifier.clone(),
        to: module_id,
      });
    }

    while let Some((path, depth)) = queue.pop_front() {
      if loaded.contains_key(&path) || sources.len() >= EXTERNAL_REACTIVITY_MAX_FILES {
        continue;
      }
      let Ok(source_text) = std::fs::read_to_string(&path) else {
        continue;
      };
      let module_id = external_module_id(root, &path);
      let language = language_for_path(&path);
      let source_text = if language == "d.ts" {
        enrich_dts_with_relative_type_imports(&path, &source_text)
      } else {
        source_text
      };
      let Ok(mut module) =
        prepare_standalone_module_source(module_id.clone(), source_text, language)
      else {
        continue;
      };
      // SummaryMerge at ExternalSummaryLoad completion (same traversal).
      if let Some(merged) = ProvisionalFactoryMergePass::run(&path, &module) {
        module = merged;
      }
      let follow =
        module.module_summary().map(|summary| summary.follow_specifiers()).unwrap_or_default();
      loaded.insert(path.clone(), module_id.clone());
      sources.push(module);

      if depth >= EXTERNAL_REACTIVITY_MAX_DEPTH {
        continue;
      }
      for specifier in follow {
        // Only follow relative / same-package paths; bare packages stay quiet.
        if !(specifier.starts_with("./") || specifier.starts_with("../")) {
          continue;
        }
        let Resolution::External { resolved_path: Some(target_path), .. } =
          resolver.resolve_from_absolute(&path, &specifier)
        else {
          continue;
        };
        let child = prefer_types_declaration(&target_path);
        let child_id = external_module_id(root, &child);
        links.push(ModuleLink { from: module_id.clone(), specifier, to: child_id.clone() });
        if !loaded.contains_key(&child) {
          queue.push_back((child, depth + 1));
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

/// Inline relative `import type` targets so same-file interface lookup sees them.
fn enrich_dts_with_relative_type_imports(dts_path: &Path, source: &str) -> String {
  let mut extras = String::new();
  let mut seen = BTreeSet::new();
  for specifier in relative_type_import_specifiers(source) {
    if !seen.insert(specifier.clone()) {
      continue;
    }
    let candidate = dts_path.parent().unwrap_or(dts_path).join(&specifier);
    let resolved = prefer_types_declaration(&candidate);
    let Ok(text) = std::fs::read_to_string(&resolved) else {
      // Try common extension swaps for `./types.js` → `types.d.ts`.
      let fallback = candidate.with_extension("").with_extension("d.ts");
      let Ok(text) = std::fs::read_to_string(&fallback) else {
        continue;
      };
      extras.push_str(&text);
      extras.push('\n');
      continue;
    };
    extras.push_str(&text);
    extras.push('\n');
  }
  if extras.is_empty() { source.to_owned() } else { format!("{extras}{source}") }
}

fn relative_type_import_specifiers(source: &str) -> Vec<String> {
  let mut out = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("import type ").unwrap_or(trimmed);
    let Some((_, from_part)) = rest.split_once(" from ") else {
      continue;
    };
    let from_part = from_part.trim().trim_end_matches(';').trim();
    let mut chars = from_part.chars();
    let Some(quote) = chars.next().filter(|ch| *ch == '"' || *ch == '\'') else {
      continue;
    };
    let rest: String = chars.collect();
    let Some((specifier, _)) = rest.split_once(quote) else {
      continue;
    };
    if specifier.starts_with("./") || specifier.starts_with("../") {
      out.push(specifier.to_owned());
    }
  }
  out
}

fn external_module_id(root: &Path, absolute: &Path) -> ModuleId {
  let relative =
    absolute.strip_prefix(root).map_or_else(|_| normalized_path(absolute), normalized_path);
  ModuleId::primary(&FileId::from(relative.as_str()))
}

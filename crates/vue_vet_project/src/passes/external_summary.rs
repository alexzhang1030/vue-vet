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
          // Bare packages stay quiet unless a `typeof` forward needs that import.
          if !is_relative && !typeof_forward_bares.contains(&specifier) {
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

/// Inline relative type-import targets so same-file interface lookup sees them.
///
/// Covers `import type { X } from './t'` and `.d.ts` `import { X } from './t'`
/// (ambient packages often omit the `type` keyword). Walks relative imports a
/// few hops (`utils → types → composables`) with a visited-path set. Successfully
/// inlined import lines are stripped to avoid duplicate bindings.
fn enrich_dts_with_relative_type_imports(dts_path: &Path, source: &str) -> String {
  let mut extras = String::new();
  let mut visited_paths = BTreeSet::new();
  let mut inlined_specifiers = BTreeSet::new();
  collect_relative_dts_extras(
    dts_path,
    source,
    0,
    &mut visited_paths,
    &mut inlined_specifiers,
    &mut extras,
  );
  if extras.is_empty() {
    return source.to_owned();
  }
  let mut kept = String::new();
  for line in source.lines() {
    if relative_import_specifier(line.trim()).is_some_and(|spec| inlined_specifiers.contains(&spec))
    {
      continue;
    }
    kept.push_str(line);
    kept.push('\n');
  }
  format!("{extras}{kept}")
}

const RELATIVE_DTS_ENRICH_MAX_DEPTH: u8 = 3;

fn collect_relative_dts_extras(
  dts_path: &Path,
  source: &str,
  depth: u8,
  visited_paths: &mut BTreeSet<PathBuf>,
  inlined_specifiers: &mut BTreeSet<String>,
  extras: &mut String,
) {
  if depth > RELATIVE_DTS_ENRICH_MAX_DEPTH {
    return;
  }
  for specifier in relative_type_import_specifiers(source) {
    let candidate = dts_path.parent().unwrap_or(dts_path).join(&specifier);
    let resolved = prefer_types_declaration(&candidate);
    let resolved = if resolved.is_file() {
      resolved
    } else {
      let fallback = candidate.with_extension("").with_extension("d.ts");
      if fallback.is_file() {
        fallback
      } else {
        continue;
      }
    };
    let Ok(canonical) = resolved.canonicalize() else {
      continue;
    };
    if !visited_paths.insert(canonical.clone()) {
      inlined_specifiers.insert(specifier);
      continue;
    }
    let Ok(text) = std::fs::read_to_string(&canonical) else {
      visited_paths.remove(&canonical);
      continue;
    };
    // Depth-first: dependants first so interfaces exist before the importer body.
    collect_relative_dts_extras(
      &canonical,
      &text,
      depth.saturating_add(1),
      visited_paths,
      inlined_specifiers,
      extras,
    );
    // Inlined bodies contribute declarations only. Stripping *all* imports avoids
    // duplicate `import { MaybeRefOrGetter } from 'vue'` across utils/types/composables
    // (Oxc semantics fails → fallback to raw utils without callback bags).
    extras.push_str(&strip_import_lines(&text));
    extras.push('\n');
    inlined_specifiers.insert(specifier);
  }
}

/// Drop `import …` lines from an inlined `.d.ts` body (relative and bare).
fn strip_import_lines(source: &str) -> String {
  let mut kept = String::new();
  for line in source.lines() {
    let trimmed = line.trim();
    if trimmed.starts_with("import ") || trimmed.starts_with("import type ") {
      continue;
    }
    kept.push_str(line);
    kept.push('\n');
  }
  kept
}

fn relative_type_import_specifiers(source: &str) -> Vec<String> {
  let mut out = Vec::new();
  for line in source.lines() {
    if let Some(specifier) = relative_import_specifier(line.trim()) {
      out.push(specifier);
    }
  }
  out
}

/// Relative specifier from `import type … from '…'` or `import {…} from '…'`.
///
/// Never `export { … } from` (barrels would concatenate every re-export target).
fn relative_import_specifier(trimmed: &str) -> Option<String> {
  let rest = if let Some(rest) = trimmed.strip_prefix("import type ") {
    rest
  } else {
    let rest = trimmed.strip_prefix("import ")?;
    // Side-effect `import './x'` has no ` from `.
    if !rest.contains(" from ") {
      return None;
    }
    rest
  };
  let (_, from_part) = rest.split_once(" from ")?;
  let from_part = from_part.trim().trim_end_matches(';').trim();
  let mut chars = from_part.chars();
  let quote = chars.next().filter(|ch| *ch == '"' || *ch == '\'')?;
  let rest: String = chars.collect();
  let (specifier, _) = rest.split_once(quote)?;
  if specifier.starts_with("./") || specifier.starts_with("../") {
    Some(specifier.to_owned())
  } else {
    None
  }
}

fn external_module_id(root: &Path, absolute: &Path) -> ModuleId {
  let relative =
    absolute.strip_prefix(root).map_or_else(|_| normalized_path(absolute), normalized_path);
  ModuleId::primary(&FileId::from(relative.as_str()))
}

/// Collapse pnpm symlink vs store paths so one package tree is loaded once.
fn canonicalize_external_path(path: &Path) -> PathBuf {
  path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Queue / budget key: `node_modules` package id, or normalized path fallback.
fn package_queue_key(path: &Path) -> String {
  node_modules_package_key(path).unwrap_or_else(|| normalized_path(path))
}

/// `node_modules/@scope/name` or `node_modules/name` key for per-package budgets.
///
/// pnpm canonical paths look like
/// `node_modules/.pnpm/@scope+name@version/node_modules/@scope/name/...` —
/// skip the `.pnpm` / `.yarn` store segment so the real package id wins
/// (otherwise every package collapses to budget key `.pnpm`).
fn node_modules_package_key(path: &Path) -> Option<String> {
  let parts: Vec<_> = path.components().collect();
  for (index, part) in parts.iter().enumerate() {
    if part.as_os_str() != "node_modules" {
      continue;
    }
    let first = parts.get(index + 1)?.as_os_str().to_str()?;
    if first == ".pnpm" || first == ".yarn" {
      continue;
    }
    if first.starts_with('@') {
      let second = parts.get(index + 2)?.as_os_str().to_str()?;
      return Some(format!("{first}/{second}"));
    }
    return Some(first.to_owned());
  }
  None
}

fn specifier_has_source_extension(specifier: &str) -> bool {
  const SUFFIXES: &[&str] = &[".d.ts", ".d.mts", ".d.cts", ".ts", ".tsx", ".js", ".mjs", ".cjs"];
  let lower = specifier.to_ascii_lowercase();
  SUFFIXES.iter().any(|suffix| lower.ends_with(suffix))
}

/// Relative re-export target as a loadable types (or JS) file.
///
/// Covers directory barrels (`./components` → `components/index.d.ts`),
/// extensionless `.d.ts` files, and types-only `./chunk.js` → `chunk.d.ts`.
fn relative_types_follow_path(importer: &Path, specifier: &str) -> Option<PathBuf> {
  let parent = importer.parent()?;
  let base = parent.join(specifier);
  let mut candidates = Vec::new();
  if base.is_file() {
    candidates.push(base.clone());
  }
  if base.is_dir() {
    for name in ["index.d.ts", "index.d.mts", "index.d.cts", "index.ts", "index.js"] {
      candidates.push(base.join(name));
    }
  }
  if !specifier_has_source_extension(specifier) {
    candidates.push(PathBuf::from(format!("{}.d.ts", base.display())));
    candidates.push(PathBuf::from(format!("{}.d.mts", base.display())));
    candidates.push(PathBuf::from(format!("{}.d.cts", base.display())));
    candidates.push(PathBuf::from(format!("{}.ts", base.display())));
  }
  if let Some(dts) = types_only_relative_declaration(importer, specifier) {
    candidates.push(dts);
  }
  for candidate in candidates {
    if candidate.is_file() {
      return Some(prefer_types_declaration(&candidate));
    }
  }
  None
}

/// `export { x } from './chunk.js'` when the package ships only `chunk.d.ts`.
fn types_only_relative_declaration(importer: &Path, specifier: &str) -> Option<PathBuf> {
  let parent = importer.parent()?;
  let candidate = parent.join(specifier);
  let file_name = candidate.file_name()?.to_str()?;
  let stem = file_name
    .strip_suffix(".mjs")
    .or_else(|| file_name.strip_suffix(".cjs"))
    .or_else(|| file_name.strip_suffix(".js"))?;
  for suffix in [".d.ts", ".d.mts", ".d.cts"] {
    let dts = candidate.with_file_name(format!("{stem}{suffix}"));
    if dts.is_file() {
      return Some(dts);
    }
  }
  None
}

#[cfg(test)]
#[expect(
  clippy::unwrap_used,
  clippy::panic,
  clippy::let_underscore_must_use,
  reason = "fixture setup in unit tests"
)]
mod enrich_fallback_tests {
  use std::{collections::BTreeSet, path::PathBuf};

  use vue_vet_core::ModuleId;
  use vue_vet_reactivity::{ModuleSource, trace_modules};

  use super::*;
  use crate::resolve::{ProjectResolver, Resolution};

  #[test]
  fn duplicate_type_import_enrichment_falls_back_to_raw_dts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-vue-query-enrich");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("node_modules/@tanstack/vue-query/build/modern")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"vue-query-enrich"}"#).unwrap();
    // Barrel that import-types two files both declaring `QueryClient` — concat fails.
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/package.json"),
      r#"{"name":"@tanstack/vue-query","types":"./build/modern/index.d.ts","exports":{".":{"types":"./build/modern/index.d.ts","import":"./build/modern/index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/index.js"),
      "export { useQuery } from './queryClient.js'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/a.d.ts"),
      "export declare class QueryClient {}\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/b.d.ts"),
      "export declare class QueryClient {}\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/queryClient.js"),
      "export function useQuery() { return {} }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/queryClient.d.ts"),
      "import type { Ref } from 'vue'\n\
       type Bag = { [K in 'data' | 'isLoading']: Ref<unknown> }\n\
       declare function useQuery(): Bag\n\
       export { useQuery as u }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/index.d.ts"),
      "import type { QueryClient as A } from './a.js'\n\
       import type { QueryClient as B } from './b.js'\n\
       export { u as useQuery } from './queryClient.js'\n\
       export type { A, B }\n",
    )
    .unwrap();
    // Companions so `import type … from './a.js'` resolve during enrich attempts.
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/a.js"),
      "export class QueryClient {}\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/b.js"),
      "export class QueryClient {}\n",
    )
    .unwrap();
    std::fs::write(
      root.join("src/direct.ts"),
      "import { computed } from 'vue'\n\
       import { useQuery } from '@tanstack/vue-query'\n\
       const { data, isLoading } = useQuery()\n\
       export const a = computed(() => data.value)\n\
       export const b = computed(() => isLoading.value)\n",
    )
    .unwrap();

    let resolver = ProjectResolver::new(&root);
    let known = BTreeSet::new();
    let Resolution::External { resolved_path: Some(path), .. } =
      resolver.resolve("src/direct.ts", "@tanstack/vue-query", &known)
    else {
      panic!("expected external resolve");
    };
    let roots = [ExternalReactivityRoot {
      from: ModuleId::from("src/direct.ts"),
      specifier: "@tanstack/vue-query".into(),
      resolved_path: path,
    }];
    let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
    assert!(!sources.is_empty(), "enriched duplicate imports must not drop the whole package");
    assert!(
      links.iter().any(|link| link.specifier.contains("queryClient")),
      "raw barrel must still follow leaf re-exports; links={links:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn typeof_forward_follows_bare_package_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-typeof-bare");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("node_modules/@ui")).unwrap();
    std::fs::create_dir_all(root.join("node_modules/field-kit")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"typeof-bare"}"#).unwrap();
    std::fs::write(
      root.join("node_modules/field-kit/package.json"),
      r#"{"name":"field-kit","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(root.join("node_modules/field-kit/index.js"), "export {}\n").unwrap();
    std::fs::write(
      root.join("node_modules/field-kit/index.d.ts"),
      "import type { Ref } from 'vue'\n\
       export interface FieldListContext { fields: Ref<{ key: string }[]> }\n\
       export declare function useFieldList(): FieldListContext\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/package.json"),
      r#"{"name":"@ui","types":"./index.d.ts","exports":{".":{"types":"./index.d.ts","import":"./index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(root.join("node_modules/@ui/index.js"), "export {}\n").unwrap();
    std::fs::write(
      root.join("node_modules/@ui/index.d.ts"),
      "import { useFieldList } from 'field-kit'\n\
       export declare const useFormFieldList: typeof useFieldList\n",
    )
    .unwrap();
    std::fs::write(
      root.join("src/consumer.ts"),
      "import { computed } from 'vue'\n\
       import { useFormFieldList } from '@ui'\n\
       const ctx = useFormFieldList()\n\
       const keys = computed(() => ctx.fields.value.map((row) => row.key))\n",
    )
    .unwrap();

    let resolver = ProjectResolver::new(&root);
    let known = BTreeSet::new();
    let Resolution::External { resolved_path: Some(path), .. } =
      resolver.resolve("src/consumer.ts", "@ui", &known)
    else {
      panic!("expected external resolve");
    };
    let roots = [ExternalReactivityRoot {
      from: ModuleId::from("src/consumer.ts"),
      specifier: "@ui".into(),
      resolved_path: path,
    }];
    let ui_path = root.join("node_modules/@ui/index.d.ts");
    let Ok(ui_module) = prepare_standalone_module_source(
      ModuleId::from("ui"),
      std::fs::read_to_string(&ui_path).unwrap(),
      "d.ts",
    ) else {
      panic!("parse ui typeof alias");
    };
    let Some(ui_summary) = ui_module.module_summary() else {
      panic!("ui summary");
    };
    let typeof_sources = ui_summary.typeof_forward_sources();
    assert!(
      typeof_sources.contains("field-kit"),
      "ui alias must publish typeof forward source; got {typeof_sources:?}"
    );
    match resolver.resolve_from_absolute(&ui_path, "field-kit") {
      Resolution::External { resolved_path: Some(path), .. } => {
        assert!(
          path.ends_with("field-kit/index.d.ts") || path.ends_with("field-kit/index.js"),
          "unexpected field-kit path {path:?}"
        );
      }
      Resolution::External { resolved_path: None, package } => {
        panic!("quiet external for typeof target {package}")
      }
      Resolution::File(path) => panic!("unexpected project file resolve {path}"),
      Resolution::Unresolved => panic!("unresolved field-kit from {ui_path:?}"),
    }

    let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
    assert!(
      sources.iter().any(|module| module.id.as_str().contains("field-kit")),
      "typeof forward must load bare package; sources={:?}",
      sources.iter().map(|module| module.id.as_str()).collect::<Vec<_>>()
    );
    assert!(
      links.iter().any(|link| link.specifier == "field-kit"),
      "typeof forward must link bare package; links={links:?}"
    );
    let mut modules = sources;
    modules.push(ModuleSource::standalone(
      "src/consumer.ts",
      std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
      "ts",
      vue_vet_core::ScriptKind::Script,
    ));
    let Ok(traced) = trace_modules(&modules, &links) else {
      panic!("trace typeof bare modules");
    };
    let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
    assert!(
      consumer.is_some_and(|module| {
        module.graph.composable_instances.contains_key("ctx")
          && module.graph.scopes.iter().any(|scope| {
            scope
              .reads
              .iter()
              .any(|read| read.binding == "fields" && read.property.as_deref() == Some("value"))
          })
      }),
      "typeof bare package must seed instance bag; got {:?}",
      consumer.map(|module| { (&module.graph.composable_instances, &module.graph.scopes) })
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn options_callback_slots_follow_package_export_star_barrel() {
    let root =
      PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-options-callback-barrel");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("node_modules/@ui/dist/types/components/Form")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"options-callback-barrel"}"#).unwrap();
    std::fs::write(
      root.join("node_modules/@ui/package.json"),
      r#"{"name":"@ui","types":"./dist/types/index.d.ts","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(root.join("node_modules/@ui/dist/index.js"), "export {}\n").unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/index.d.ts"),
      "export * from './components'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/index.d.ts"),
      "export * from './Form'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/Form/index.d.ts"),
      "export { defineStdFormProps } from './utils'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/Form/composables.d.ts"),
      "import type { Ref } from 'vue'\n\
       export interface StdFormContext { values: Ref<unknown>; form: unknown }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/Form/types.d.ts"),
      "import { StdFormContext } from './composables'\n\
       export interface StdFormGlobalSetupContext extends StdFormContext { schema: unknown }\n\
       export type StdFormGlobalSetupFn = (ctx: StdFormGlobalSetupContext) => unknown\n\
       export interface StdFormProps<Setup extends StdFormGlobalSetupFn> { setup?: Setup }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/Form/utils.d.ts"),
      "import { StdFormGlobalSetupFn, StdFormProps } from './types'\n\
       export declare function defineStdFormProps<Setup extends StdFormGlobalSetupFn>(\n\
         props: StdFormProps<Setup>,\n\
       ): StdFormProps<Setup>\n",
    )
    .unwrap();
    std::fs::write(
      root.join("src/consumer.ts"),
      "import { computed } from 'vue'\n\
       import { defineStdFormProps } from '@ui'\n\
       defineStdFormProps({\n\
         setup: ({ values }) => computed(() => values.value),\n\
       })\n",
    )
    .unwrap();

    let resolver = ProjectResolver::new(&root);
    let known = BTreeSet::new();
    let Resolution::External { resolved_path: Some(path), .. } =
      resolver.resolve("src/consumer.ts", "@ui", &known)
    else {
      panic!("expected external resolve");
    };
    let roots = [ExternalReactivityRoot {
      from: ModuleId::from("src/consumer.ts"),
      specifier: "@ui".into(),
      resolved_path: path,
    }];
    let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
    assert!(
      sources.iter().any(|source| source.id.as_str().contains("Form/utils")),
      "package follow must load Form/utils.d.ts; sources={:?}",
      sources.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
    );
    let mut modules = sources;
    modules.push(ModuleSource::standalone(
      "src/consumer.ts",
      std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
      "ts",
      vue_vet_core::ScriptKind::Script,
    ));
    let Ok(traced) = trace_modules(&modules, &links) else {
      panic!("trace options-callback barrel modules");
    };
    let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
    assert!(
      consumer.is_some_and(|module| {
        module.graph.scopes.iter().any(|scope| {
          scope.reads.iter().any(|read| read.binding == "values")
            && scope.uncertain_accesses.is_empty()
        })
      }),
      "export* barrel must surface options-callback slots; got {:?}",
      consumer.map(|module| &module.graph.scopes)
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn enrich_strips_bare_imports_so_duplicate_vue_types_still_seed() {
    // Real UI Form chain: utils/types/composables each `import { MaybeRefOrGetter } from 'vue'`.
    let root =
      PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-dts-dup-vue-import");
    let _ = std::fs::remove_dir_all(&root);
    let form = root.join("node_modules/@ui/dist/types/components/Form");
    std::fs::create_dir_all(&form).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"dup-vue-import"}"#).unwrap();
    std::fs::write(
      root.join("node_modules/@ui/package.json"),
      r#"{"name":"@ui","types":"./dist/types/index.d.ts","exports":{".":{"types":"./dist/types/index.d.ts","import":"./dist/index.js"}}}"#,
    )
    .unwrap();
    std::fs::write(root.join("node_modules/@ui/dist/index.js"), "export {}\n").unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/index.d.ts"),
      "export * from './components'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@ui/dist/types/components/index.d.ts"),
      "export * from './Form'\n",
    )
    .unwrap();
    std::fs::write(form.join("index.d.ts"), "export { defineStdFormProps } from './utils'\n")
      .unwrap();
    std::fs::write(
      form.join("composables.d.ts"),
      "import { MaybeRefOrGetter, Ref } from 'vue'\n\
       export interface StdFormContext { values: Ref<unknown>; form: unknown }\n",
    )
    .unwrap();
    std::fs::write(
      form.join("types.d.ts"),
      "import { MaybeRefOrGetter } from 'vue'\n\
       import { StdFormContext } from './composables'\n\
       export interface StdFormGlobalSetupContext extends StdFormContext {}\n\
       export type StdFormGlobalSetupFn = (ctx: StdFormGlobalSetupContext) => unknown\n\
       export interface StdFormProps<Setup extends StdFormGlobalSetupFn> { setup?: Setup }\n",
    )
    .unwrap();
    std::fs::write(
      form.join("utils.d.ts"),
      "import { MaybeRefOrGetter } from 'vue'\n\
       import { StdFormGlobalSetupFn, StdFormProps } from './types'\n\
       export declare function defineStdFormProps<Setup extends StdFormGlobalSetupFn>(\n\
         props: StdFormProps<Setup>,\n\
       ): StdFormProps<Setup>\n",
    )
    .unwrap();
    std::fs::write(
      root.join("src/consumer.ts"),
      "import { computed } from 'vue'\n\
       import { defineStdFormProps } from '@ui'\n\
       defineStdFormProps({\n\
         setup: ({ values }) => computed(() => values.value),\n\
       })\n",
    )
    .unwrap();

    let utils = form.join("utils.d.ts");
    let enriched =
      enrich_dts_with_relative_type_imports(&utils, &std::fs::read_to_string(&utils).unwrap());
    assert!(
      prepare_standalone_module_source(ModuleId::from("utils.d.ts"), enriched.clone(), "d.ts")
        .is_ok(),
      "enrich must strip duplicate vue imports; enriched:\n{enriched}"
    );

    let resolver = ProjectResolver::new(&root);
    let known = BTreeSet::new();
    let Resolution::External { resolved_path: Some(path), .. } =
      resolver.resolve("src/consumer.ts", "@ui", &known)
    else {
      panic!("expected external resolve");
    };
    let roots = [ExternalReactivityRoot {
      from: ModuleId::from("src/consumer.ts"),
      specifier: "@ui".into(),
      resolved_path: path,
    }];
    let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
    let mut modules = sources;
    modules.push(ModuleSource::standalone(
      "src/consumer.ts",
      std::fs::read_to_string(root.join("src/consumer.ts")).unwrap(),
      "ts",
      vue_vet_core::ScriptKind::Script,
    ));
    let Ok(traced) = trace_modules(&modules, &links) else {
      panic!("trace dup-vue-import modules");
    };
    let consumer = traced.iter().find(|module| module.id.as_str() == "src/consumer.ts");
    assert!(
      consumer.is_some_and(|module| {
        module.graph.scopes.iter().any(|scope| {
          scope.reads.iter().any(|read| read.binding == "values")
            && scope.uncertain_accesses.is_empty()
        })
      }),
      "duplicate vue imports must not block options-callback seeds; got {:?}",
      consumer.map(|module| &module.graph.scopes)
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn pnpm_store_path_uses_real_package_budget_key() {
    let path = PathBuf::from(
      "/proj/node_modules/.pnpm/@standard-design+ui@1.0.0/node_modules/@standard-design/ui/dist/types/index.d.ts",
    );
    assert_eq!(
      node_modules_package_key(&path).as_deref(),
      Some("@standard-design/ui"),
      "pnpm store paths must not budget under `.pnpm`"
    );
    let plain = PathBuf::from("/proj/node_modules/vue/dist/vue.d.ts");
    assert_eq!(node_modules_package_key(&plain).as_deref(), Some("vue"));
  }

  #[test]
  fn relative_value_import_in_dts_inlines_and_strips() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-dts-value-import");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("pkg")).unwrap();
    std::fs::write(
      root.join("pkg/types.d.ts"),
      "import type { Ref } from 'vue'\n\
       export interface Ctx { values: Ref<number> }\n\
       export type SetupFn = (ctx: Ctx) => void\n\
       export interface Props<S extends SetupFn> { setup?: S }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("pkg/utils.d.ts"),
      "import { SetupFn, Props } from './types'\n\
       export declare function defineFormProps<S extends SetupFn>(props: Props<S>): void\n",
    )
    .unwrap();
    let enriched = enrich_dts_with_relative_type_imports(
      &root.join("pkg/utils.d.ts"),
      &std::fs::read_to_string(root.join("pkg/utils.d.ts")).unwrap(),
    );
    assert!(
      enriched.contains("interface Ctx")
        && enriched.contains("defineFormProps")
        && !enriched.contains("from './types'"),
      "must inline './types' and strip the import; got:\n{enriched}"
    );
    let _ = std::fs::remove_dir_all(&root);
  }

  #[test]
  fn types_only_chunk_reexport_loads_use_query_seed() {
    // Packaged barrels often re-export `./queryClient-HASH.js` when only `.d.ts` ships.
    let root =
      PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp-vue-query-types-only");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("node_modules/@tanstack/vue-query/build/modern")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("package.json"), r#"{"name":"vue-query-types-only"}"#).unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/package.json"),
      r#"{"name":"@tanstack/vue-query","types":"./build/modern/index.d.ts","exports":{".":{"types":"./build/modern/index.d.ts","import":"./build/modern/index.js"}}}"#,
    )
    .unwrap();
    // Entry `.js` exists so the package resolves; the leaf chunk is types-only.
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/index.js"),
      "export { useQuery } from './queryClient-HASH.js'\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/queryClient-HASH.d.ts"),
      "import type { Ref } from 'vue'\n\
       type Bag = { [K in 'data' | 'isLoading']: Ref<unknown> }\n\
       declare function useQuery(): Bag\n\
       export { useQuery as u }\n",
    )
    .unwrap();
    std::fs::write(
      root.join("node_modules/@tanstack/vue-query/build/modern/index.d.ts"),
      "export { u as useQuery } from './queryClient-HASH.js'\n",
    )
    .unwrap();

    let resolver = ProjectResolver::new(&root);
    let known = BTreeSet::new();
    let Resolution::External { resolved_path: Some(path), .. } =
      resolver.resolve("src/consumer.ts", "@tanstack/vue-query", &known)
    else {
      panic!("expected external resolve");
    };
    let roots = [ExternalReactivityRoot {
      from: ModuleId::from("consumer.ts"),
      specifier: "@tanstack/vue-query".into(),
      resolved_path: path,
    }];
    let (sources, links) = ExternalSummaryLoadPass::run(&root, &resolver, &roots, None);
    assert!(!sources.is_empty(), "vue-query index/leaves must load");
    assert!(
      sources.iter().any(|source| source.id.as_str().contains("queryClient-HASH")),
      "types-only chunk follow must load queryClient-HASH.d.ts; sources={:?}",
      sources.iter().map(|source| source.id.as_str()).collect::<Vec<_>>()
    );
    let mut modules = sources;
    modules.push(ModuleSource::standalone(
      "consumer.ts",
      "import { computed } from 'vue';\n\
       import { useQuery } from '@tanstack/vue-query';\n\
       const { data, isLoading } = useQuery({ queryKey: ['x'] as const, queryFn: () => Promise.resolve(1) });\n\
       export const a = computed(() => data.value);\n\
       export const b = computed(() => isLoading.value);\n",
      "ts",
      vue_vet_core::ScriptKind::Script,
    ));
    let Ok(traced) = trace_modules(&modules, &links) else {
      panic!("trace types-only vue-query modules");
    };
    let consumer = traced.iter().find(|module| module.id.as_str() == "consumer.ts");
    assert!(
      consumer.is_some_and(|module| {
        module.graph.bindings.iter().any(|binding| binding.name == "data")
          && module.graph.scopes.iter().any(|scope| {
            scope.reads.iter().any(|read| read.binding == "data")
              && scope.uncertain_accesses.is_empty()
          })
      }),
      "types-only package useQuery must seed; got {:?}",
      consumer.map(|module| (&module.graph.bindings, &module.graph.scopes))
    );
    let _ = std::fs::remove_dir_all(&root);
  }
}

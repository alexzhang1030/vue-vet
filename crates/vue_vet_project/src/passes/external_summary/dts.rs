//! Inline relative type-imports into a `.d.ts` body for same-file lookup.
use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use crate::resolve::prefer_types_declaration;

/// Inline relative type-import targets so same-file interface lookup sees them.
///
/// Covers `import type { X } from './t'` and `.d.ts` `import { X } from './t'`
/// (ambient packages often omit the `type` keyword). Walks relative imports a
/// few hops (`utils → types → composables`) with a visited-path set. Successfully
/// inlined import lines are stripped to avoid duplicate bindings.
pub fn enrich_dts_with_relative_type_imports(dts_path: &Path, source: &str) -> String {
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

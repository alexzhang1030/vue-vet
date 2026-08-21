//! External package path / budget keys and relative types follow.
use std::path::{Path, PathBuf};

use vue_vet_core::{FileId, ModuleId};

use crate::resolve::{normalized_path, prefer_types_declaration};

pub fn external_module_id(root: &Path, absolute: &Path) -> ModuleId {
  let relative =
    absolute.strip_prefix(root).map_or_else(|_| normalized_path(absolute), normalized_path);
  ModuleId::primary(&FileId::from(relative.as_str()))
}

/// Collapse pnpm symlink vs store paths so one package tree is loaded once.
pub fn canonicalize_external_path(path: &Path) -> PathBuf {
  path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Queue / budget key: `node_modules` package id, or normalized path fallback.
pub fn package_queue_key(path: &Path) -> String {
  node_modules_package_key(path).unwrap_or_else(|| normalized_path(path))
}

/// `node_modules/@scope/name` or `node_modules/name` key for per-package budgets.
///
/// pnpm canonical paths look like
/// `node_modules/.pnpm/@scope+name@version/node_modules/@scope/name/...` —
/// skip the `.pnpm` / `.yarn` store segment so the real package id wins
/// (otherwise every package collapses to budget key `.pnpm`).
pub fn node_modules_package_key(path: &Path) -> Option<String> {
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
pub fn relative_types_follow_path(importer: &Path, specifier: &str) -> Option<PathBuf> {
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

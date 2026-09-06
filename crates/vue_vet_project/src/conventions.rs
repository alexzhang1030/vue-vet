//! Nuxt-style component naming without executing `nuxt.config`.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Component, Path},
};

use vue_vet_oxc::{NuxtContentModulePolicy, parse_nuxt_config};

use crate::resolve::{normalize_project_root, normalized_path};

/// Strip Nuxt mode / visibility suffixes from a component file stem.
///
/// Mirrors Nuxt's `MODE_REPLACEMENT_RE`:
/// `(?:\.(?:client|server))?(?:\.global|\.island)*$`
#[must_use]
pub fn strip_nuxt_component_suffixes(stem: &str) -> String {
  let mut name = stem.to_owned();
  loop {
    let lower = name.to_ascii_lowercase();
    if let Some(stripped) = lower
      .strip_suffix(".island")
      .or_else(|| lower.strip_suffix(".global"))
      .or_else(|| lower.strip_suffix(".client"))
      .or_else(|| lower.strip_suffix(".server"))
    {
      name.truncate(stripped.len());
      continue;
    }
    break;
  }
  name
}

/// Derive the Nuxt auto-import `PascalCase` name for a path under `components/`.
///
/// Defaults assume `pathPrefix: true`. Custom dirs / `pathPrefix: false` need
/// `.nuxt/components.d.ts` enrichment.
#[must_use]
pub fn convention_component_name(path: &str) -> Option<String> {
  convention_component_name_with_content(path, false)
}

/// Like [`convention_component_name`], dropping the Nuxt Content `content/`
/// path prefix (`pathPrefix: false`, `prefix: ''`) when that module is present.
#[must_use]
pub fn convention_component_name_with_content(path: &str, nuxt_content: bool) -> Option<String> {
  let relative = path_under_components(path)?;
  let path = Path::new(relative);
  // Nuxt Content `pathPrefix: false` drops every relative directory under the
  // framework-owned `components/content/` tree. Ordinary `…/widgets/content/`
  // paths keep their prefixes.
  let prefix_parts = if nuxt_content {
    Vec::new()
  } else {
    path
      .parent()
      .map(|parent| {
        parent
          .components()
          .filter_map(|component| match component {
            Component::Normal(part) => part.to_str().map(str::to_owned),
            _ => None,
          })
          .filter(|part| !is_grouping_folder(part))
          .collect::<Vec<_>>()
      })
      .unwrap_or_default()
  };

  let mut file_name = path.file_stem().and_then(|name| name.to_str()).unwrap_or("").to_owned();
  file_name = strip_nuxt_component_suffixes(&file_name);
  if file_name.eq_ignore_ascii_case("index") {
    file_name.clear();
  }

  // Drop quote-like characters Nuxt also strips.
  file_name = file_name.replace(['\'', '"', '`'], "");

  let segments = resolve_component_name_segments(&file_name, &prefix_parts);
  let pascal = pascal_case(&segments);
  if pascal.is_empty() { None } else { Some(pascal) }
}

/// If `tag` looks like Nuxt's `Lazy*` auto-import, return the base name.
#[must_use]
pub fn strip_lazy_component_prefix(tag: &str) -> Option<&str> {
  let rest = tag.strip_prefix("Lazy")?;
  let first = rest.chars().next()?;
  if first.is_ascii_uppercase() { Some(rest) } else { None }
}

/// Parse Nuxt-generated component declaration maps into `name -> project path`.
///
/// Accepts `.nuxt/components.d.ts` and `.nuxt/types/components.d.ts` shapes.
#[must_use]
pub fn parse_nuxt_components_dts(
  dts_path: &Path,
  source: &str,
  root: &Path,
  known: &BTreeSet<String>,
) -> BTreeMap<String, String> {
  let mut names = BTreeMap::new();
  let root = normalize_project_root(root);
  let dts_dir = dts_path.parent().map_or_else(|| root.clone(), Path::to_path_buf);
  for (name, import_path) in extract_typeof_imports(source) {
    if name.starts_with("Lazy") && name.chars().nth(4).is_some_and(|ch| ch.is_ascii_uppercase()) {
      continue;
    }
    let absolute = if Path::new(&import_path).is_absolute() {
      Path::new(&import_path).to_path_buf()
    } else {
      dts_dir.join(&import_path)
    };
    let absolute = normalize_project_root(&absolute);
    let Some(relative) = absolute.strip_prefix(&root).ok().map(normalized_path) else {
      continue;
    };
    if known.contains(&relative) {
      names.insert(name, relative);
    }
  }
  names
}

/// Candidate dts files relative to the project root (deterministic order).
pub const NUXT_COMPONENT_DTS_CANDIDATES: &[&str] =
  &[".nuxt/components.d.ts", ".nuxt/types/components.d.ts"];

/// Bare auto-import maps (Nuxt + Vite unplugin-auto-import).
///
/// Order matters: prefer `.nuxt/imports.d.ts` re-exports when both Nuxt maps
/// exist; Vite `auto-imports.d.ts` is last so Nuxt wins on name collisions.
pub const NUXT_IMPORTS_DTS_CANDIDATES: &[&str] =
  &[".nuxt/imports.d.ts", ".nuxt/types/imports.d.ts", "auto-imports.d.ts", "src/auto-imports.d.ts"];

/// Synthetic `ModuleLink` specifier prefix for bare Nuxt / Vite auto-import calls.
///
/// Format: `#nuxt-imports:{exportName}` — reactivity seed only (never unresolved-import).
pub const NUXT_IMPORTS_SPECIFIER_PREFIX: &str = "#nuxt-imports:";

/// One bare auto-import binding from a Nuxt or Vite imports map.
///
/// Specifiers are relative to [`Self::importer`] (the dts that declared them),
/// not to the consumer SFC. `.nuxt/types/imports.d.ts` uses one more `../`
/// than `.nuxt/imports.d.ts` for the same package. Vite root
/// `auto-imports.d.ts` typically uses `./src/…` from the project root.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NuxtImportTarget {
  pub specifier: String,
  /// Workspace-relative path of the declaring dts.
  pub importer: String,
}

#[must_use]
pub fn load_nuxt_component_dts_names(
  root: &Path,
  known: &BTreeSet<String>,
) -> BTreeMap<String, String> {
  let mut names = BTreeMap::new();
  for candidate in NUXT_COMPONENT_DTS_CANDIDATES {
    let path = root.join(candidate);
    let Ok(source) = std::fs::read_to_string(&path) else {
      continue;
    };
    for (name, relative) in parse_nuxt_components_dts(&path, &source, root, known) {
      names.insert(name, relative);
    }
  }
  names
}

/// Parse Nuxt / unplugin-auto-import maps into `name -> specifier`.
///
/// Accepts `export { useX } from '…'`, `typeof import('…').useX`, and
/// `typeof import('…')['useX']` (Vite `auto-imports.d.ts`) shapes.
#[must_use]
pub fn parse_nuxt_imports_dts(source: &str) -> BTreeMap<String, String> {
  let mut names = BTreeMap::new();
  for (name, specifier) in extract_named_reexports(source) {
    names.insert(name, specifier);
  }
  for (name, specifier) in extract_typeof_member_imports(source) {
    names.entry(name).or_insert(specifier);
  }
  names
}

#[must_use]
pub fn load_nuxt_imports_dts_names(root: &Path) -> BTreeMap<String, NuxtImportTarget> {
  let mut names = BTreeMap::new();
  for candidate in NUXT_IMPORTS_DTS_CANDIDATES {
    let path = root.join(candidate);
    let Ok(source) = std::fs::read_to_string(&path) else {
      continue;
    };
    for (name, specifier) in parse_nuxt_imports_dts(&source) {
      // First candidate wins: Nuxt maps before Vite `auto-imports.d.ts`.
      names
        .entry(name)
        .or_insert_with(|| NuxtImportTarget { specifier, importer: (*candidate).to_owned() });
    }
  }
  names
}

#[must_use]
pub fn nuxt_imports_link_specifier(export_name: &str) -> String {
  format!("{NUXT_IMPORTS_SPECIFIER_PREFIX}{export_name}")
}

fn path_under_components(path: &str) -> Option<&str> {
  const MARKER: &str = "/components/";
  let relative = match path.rfind(MARKER) {
    Some(index) => path.get(index.saturating_add(MARKER.len())..)?,
    None => path.strip_prefix("components/")?,
  };
  if relative.is_empty() { None } else { Some(relative) }
}

fn is_grouping_folder(segment: &str) -> bool {
  let trimmed = segment.trim();
  trimmed.starts_with('(') && trimmed.ends_with(')')
}

/// Framework-owned Content component dirs relative to a package/config owner.
const NUXT_CONTENT_COMPONENT_PREFIXES: &[&str] =
  &["components/content/", "app/components/content/", "src/components/content/"];

/// `app/components/content/…` (and srcDir-equivalent) under the nearest owner.
///
/// Package/config directories are ownership boundaries. A parent Content
/// registration does not leak into a nearer ordinary package, and a Content
/// root never matches an unrelated sibling package path.
#[must_use]
pub fn is_nuxt_content_component(
  path: &str,
  content_roots: &BTreeSet<String>,
  owners: &BTreeSet<String>,
  src_dirs: &BTreeMap<String, String>,
) -> bool {
  if content_roots.is_empty() {
    return false;
  }
  let normalized = path.replace('\\', "/");
  let Some(owner) = nearest_owner(&normalized, owners, content_roots) else {
    return false;
  };
  if !content_roots.contains(owner) {
    return false;
  }
  let relative = relative_to_owner(&normalized, owner);
  content_component_prefixes(src_dirs.get(owner).map(String::as_str))
    .iter()
    .any(|prefix| relative.starts_with(prefix))
}

fn content_component_prefixes(src_dir: Option<&str>) -> Vec<String> {
  match src_dir {
    Some(dir) if !dir.is_empty() && dir != "." => {
      let trimmed = dir.trim_matches('/');
      vec![format!("{trimmed}/components/content/")]
    }
    _ => NUXT_CONTENT_COMPONENT_PREFIXES.iter().map(|prefix| (*prefix).to_owned()).collect(),
  }
}

#[must_use]
pub fn nearest_owner<'a>(
  path: &str,
  owners: &'a BTreeSet<String>,
  content_roots: &'a BTreeSet<String>,
) -> Option<&'a str> {
  owners
    .iter()
    .chain(content_roots.iter())
    .filter(|owner| path_is_under(path, owner))
    .max_by_key(|owner| owner.len())
    .map(String::as_str)
}

fn path_is_under(path: &str, owner: &str) -> bool {
  if owner.is_empty() {
    return true;
  }
  path == owner || path.starts_with(&format!("{owner}/"))
}

fn relative_to_owner<'a>(path: &'a str, owner: &str) -> &'a str {
  if owner.is_empty() {
    return path;
  }
  path.strip_prefix(owner).and_then(|rest| rest.strip_prefix('/')).unwrap_or(path)
}

/// Combine package-dependency evidence with a structured Nuxt `modules` policy.
#[must_use]
pub const fn owner_enables_nuxt_content(
  package_has_dep: bool,
  config_policy: NuxtContentModulePolicy,
) -> bool {
  match config_policy {
    NuxtContentModulePolicy::IncludesContent => true,
    NuxtContentModulePolicy::Empty => false,
    NuxtContentModulePolicy::Absent | NuxtContentModulePolicy::Unresolved => package_has_dep,
  }
}

/// Structured enablement for one package.json or `nuxt.config.*` source.
#[must_use]
pub fn source_nuxt_content_evidence(path: &str, source: &str) -> OwnerContentEvidence {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  if name == "package.json" {
    return OwnerContentEvidence {
      package_has_dep: package_json_has_nuxt_content(source),
      config_policy: NuxtContentModulePolicy::Absent,
    };
  }
  if is_nuxt_config_file(name) {
    return OwnerContentEvidence {
      package_has_dep: false,
      config_policy: parse_nuxt_config(path, source).modules,
    };
  }
  OwnerContentEvidence::default()
}

/// Package.json / `nuxt.config.*` facts used while discovering Content owners.
#[must_use]
pub fn source_owner_config_facts(path: &str, source: &str) -> OwnerConfigFacts {
  let name = Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or(path);
  if name == "package.json" {
    return OwnerConfigFacts {
      evidence: source_nuxt_content_evidence(path, source),
      src_dir: None,
      extends: Vec::new(),
    };
  }
  if is_nuxt_config_file(name) {
    let parsed = parse_nuxt_config(path, source);
    return OwnerConfigFacts {
      evidence: OwnerContentEvidence { package_has_dep: false, config_policy: parsed.modules },
      src_dir: parsed.src_dir,
      extends: parsed.extends,
    };
  }
  OwnerConfigFacts::default()
}

#[derive(Clone, Debug, Default)]
pub struct OwnerConfigFacts {
  pub evidence: OwnerContentEvidence,
  pub src_dir: Option<String>,
  pub extends: Vec<String>,
}

impl OwnerConfigFacts {
  pub fn merge(&self, other: Self) -> Self {
    let mut merged = self.clone();
    merged.evidence = merged.evidence.merge(other.evidence);
    if other.src_dir.is_some() {
      merged.src_dir = other.src_dir;
    }
    if !other.extends.is_empty() {
      merged.extends = other.extends;
    }
    merged
  }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OwnerContentEvidence {
  pub package_has_dep: bool,
  pub config_policy: NuxtContentModulePolicy,
}

impl OwnerContentEvidence {
  pub const fn merge(mut self, other: Self) -> Self {
    self.package_has_dep |= other.package_has_dep;
    self.config_policy = merge_config_policy(self.config_policy, other.config_policy);
    self
  }

  #[must_use]
  pub const fn enables(self) -> bool {
    owner_enables_nuxt_content(self.package_has_dep, self.config_policy)
  }
}

const fn merge_config_policy(
  current: NuxtContentModulePolicy,
  next: NuxtContentModulePolicy,
) -> NuxtContentModulePolicy {
  match (current, next) {
    (NuxtContentModulePolicy::IncludesContent, _)
    | (_, NuxtContentModulePolicy::IncludesContent) => NuxtContentModulePolicy::IncludesContent,
    (NuxtContentModulePolicy::Empty, _) | (_, NuxtContentModulePolicy::Empty) => {
      NuxtContentModulePolicy::Empty
    }
    (NuxtContentModulePolicy::Unresolved, _) | (_, NuxtContentModulePolicy::Unresolved) => {
      NuxtContentModulePolicy::Unresolved
    }
    _ => NuxtContentModulePolicy::Absent,
  }
}

#[must_use]
pub fn is_nuxt_config_file(name: &str) -> bool {
  matches!(
    name,
    "nuxt.config.ts" | "nuxt.config.js" | "nuxt.config.mjs" | "nuxt.config.mts" | "nuxt.config.cjs"
  )
}

fn package_json_has_nuxt_content(source: &str) -> bool {
  let Ok(value) = serde_json::from_str::<serde_json::Value>(source) else {
    return false;
  };
  for field in ["dependencies", "devDependencies", "optionalDependencies"] {
    if value.get(field).and_then(|deps| deps.get("@nuxt/content")).is_some() {
      return true;
    }
  }
  false
}

#[must_use]
pub fn parent_dir_key(relative: &str) -> String {
  Path::new(relative).parent().map(normalized_path).unwrap_or_default()
}

fn resolve_component_name_segments(file_name: &str, prefix_parts: &[String]) -> Vec<String> {
  let file_name_parts = split_by_case(file_name);
  let file_name_parts_content = file_name_parts.join("/").to_ascii_lowercase();
  let mut component_name_parts =
    prefix_parts.iter().flat_map(|part| split_by_case(part)).collect::<Vec<_>>();
  let mut matched_suffix = Vec::new();
  for (index, prefix_part) in prefix_parts.iter().enumerate().rev() {
    let mut prefix_cases = split_by_case(prefix_part);
    prefix_cases.reverse();
    for part in prefix_cases {
      matched_suffix.insert(0, part.to_ascii_lowercase());
    }
    let matched_suffix_content = matched_suffix.join("/");
    let prefix_eq_file = prefix_part.eq_ignore_ascii_case(&file_name_parts_content);
    let next_duplicates =
      prefix_parts.get(index.saturating_add(1)).is_some_and(|next| next == prefix_part);
    if file_name_parts_content == matched_suffix_content
      || file_name_parts_content.starts_with(&(matched_suffix_content.clone() + "/"))
      || (prefix_eq_file && next_duplicates)
    {
      component_name_parts.truncate(index);
    }
  }
  component_name_parts.extend(file_name_parts);
  component_name_parts
}

fn split_by_case(input: &str) -> Vec<String> {
  if input.is_empty() {
    return Vec::new();
  }
  let mut parts = Vec::new();
  let mut current = String::new();
  let chars = input.chars().collect::<Vec<_>>();
  for (index, &ch) in chars.iter().enumerate() {
    if matches!(ch, '-' | '_' | '/' | '.') {
      if !current.is_empty() {
        parts.push(std::mem::take(&mut current));
      }
      continue;
    }
    let prev = index.checked_sub(1).and_then(|i| chars.get(i).copied());
    let next = chars.get(index + 1).copied();
    let boundary = ch.is_ascii_uppercase() && prev.is_some_and(|prev| prev.is_ascii_lowercase())
      || (ch.is_ascii_uppercase()
        && prev.is_some_and(|prev| prev.is_ascii_uppercase())
        && next.is_some_and(|next| next.is_ascii_lowercase()));
    if boundary && !current.is_empty() {
      parts.push(std::mem::take(&mut current));
    }
    current.push(ch);
  }
  if !current.is_empty() {
    parts.push(current);
  }
  parts
}

fn pascal_case(parts: &[String]) -> String {
  parts
    .iter()
    .filter(|part| !part.is_empty())
    .map(|part| {
      let mut chars = part.chars();
      chars.next().map_or_else(String::new, |first| {
        let mut out = first.to_ascii_uppercase().to_string();
        out.extend(chars.flat_map(char::to_lowercase));
        out
      })
    })
    .collect()
}

fn extract_typeof_imports(source: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    let Some((before, after_marker)) = trimmed.split_once("typeof import(") else {
      continue;
    };
    let Some(name) = component_name_before_colon(before.trim_end()) else {
      continue;
    };
    let mut chars = after_marker.chars();
    let Some(quote) = chars.next().filter(|ch| *ch == '"' || *ch == '\'') else {
      continue;
    };
    let rest: String = chars.collect();
    let Some((import_path, _)) = rest.split_once(quote) else {
      continue;
    };
    if !name.is_empty() && !import_path.is_empty() {
      out.push((name, import_path.to_owned()));
    }
  }
  out
}

/// `export { useColorMode } from '…/composables'` / multi-name lists.
fn extract_named_reexports(source: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim().trim_end_matches(';').trim();
    let Some(rest) = trimmed.strip_prefix("export {") else {
      continue;
    };
    let Some((names_part, from_part)) = rest.split_once("} from ") else {
      continue;
    };
    let from_part = from_part.trim();
    let mut chars = from_part.chars();
    let Some(quote) = chars.next().filter(|ch| *ch == '"' || *ch == '\'') else {
      continue;
    };
    let rest: String = chars.collect();
    let Some((import_path, _)) = rest.split_once(quote) else {
      continue;
    };
    if import_path.is_empty() {
      continue;
    }
    for piece in names_part.split(',') {
      let piece = piece.trim();
      if piece.is_empty() {
        continue;
      }
      // `useX as Y` → local auto-import name is Y.
      let name = if let Some((original, alias)) = piece.split_once(" as ") {
        let alias = alias.trim();
        if alias.is_empty() { original.trim() } else { alias }
      } else {
        piece
      };
      if name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
        out.push((name.to_owned(), import_path.to_owned()));
      }
    }
  }
  out
}

/// `typeof import('…').useX` (Nuxt types) and `typeof import('…')['useX']`
/// (Vite unplugin-auto-import).
fn extract_typeof_member_imports(source: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  for line in source.lines() {
    let trimmed = line.trim();
    let Some((before, after_marker)) = trimmed.split_once("typeof import(") else {
      continue;
    };
    let Some(name) = component_name_before_colon(before.trim_end()) else {
      continue;
    };
    let mut chars = after_marker.chars();
    let Some(quote) = chars.next().filter(|ch| *ch == '"' || *ch == '\'') else {
      continue;
    };
    let rest: String = chars.collect();
    let Some((import_path, after_path)) = rest.split_once(quote) else {
      continue;
    };
    // Require a member access so component maps (`typeof import('x')`) stay elsewhere.
    let after_path = after_path.trim_start();
    if !after_path.starts_with(')') {
      continue;
    }
    let after_paren = after_path.trim_start_matches(')').trim_start();
    if after_paren.is_empty() || after_paren.starts_with(';') {
      continue;
    }
    if let Some(member) = after_paren.strip_prefix('.') {
      let member =
        member.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_').next().unwrap_or("");
      if member.is_empty() {
        continue;
      }
      // Keep the declaration's local name (left of `:`) as the auto-import key.
    } else if after_paren.starts_with('[') {
      if bracket_member_name(after_paren).is_none() {
        continue;
      }
    } else {
      continue;
    }
    if !name.is_empty() && !import_path.is_empty() {
      out.push((name, import_path.to_owned()));
    }
  }
  out
}

/// `['useX']` / `["useX"]` after `typeof import('…')`.
fn bracket_member_name(after_paren: &str) -> Option<&str> {
  let rest = after_paren.strip_prefix('[')?.trim_start();
  let quote = rest.chars().next().filter(|ch| *ch == '"' || *ch == '\'')?;
  let inner = rest.get(1..)?;
  let (member, after_member) = inner.split_once(quote)?;
  let after_member = after_member.trim_start();
  if !after_member.starts_with(']') {
    return None;
  }
  if member.is_empty() || !member.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
    return None;
  }
  Some(member)
}

fn component_name_before_colon(before: &str) -> Option<String> {
  let before = before.strip_prefix("export const ").unwrap_or(before);
  let before = before.trim_end_matches(':').trim();
  let name = before.split_whitespace().next_back()?.trim();
  if name.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
    Some(name.to_owned())
  } else {
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn strips_client_server_global_island_suffixes() {
    assert_eq!(strip_nuxt_component_suffixes("HeroDemo.client"), "HeroDemo");
    assert_eq!(strip_nuxt_component_suffixes("Panel.server"), "Panel");
    assert_eq!(strip_nuxt_component_suffixes("Widget.global"), "Widget");
    assert_eq!(strip_nuxt_component_suffixes("Isle.island"), "Isle");
    assert_eq!(strip_nuxt_component_suffixes("Mixed.client.global"), "Mixed");
  }

  #[test]
  fn derives_nested_and_index_names() {
    assert_eq!(
      convention_component_name("components/HeroDemo.client.vue").as_deref(),
      Some("HeroDemo")
    );
    assert_eq!(
      convention_component_name("app/components/base/Button.vue").as_deref(),
      Some("BaseButton")
    );
    assert_eq!(convention_component_name("components/ui/index.vue").as_deref(), Some("Ui"));
    assert_eq!(
      convention_component_name("components/base/BaseButton.vue").as_deref(),
      Some("BaseButton")
    );
    assert_eq!(
      convention_component_name("app/components/content/GuidePanel.vue").as_deref(),
      Some("ContentGuidePanel")
    );
    assert_eq!(
      convention_component_name_with_content("app/components/content/GuidePanel.vue", true)
        .as_deref(),
      Some("GuidePanel")
    );
    assert_eq!(
      convention_component_name_with_content("app/components/content/nested/GuidePanel.vue", true)
        .as_deref(),
      Some("GuidePanel")
    );
    assert_eq!(
      convention_component_name("app/components/widgets/content/GuidePanel.vue").as_deref(),
      Some("WidgetsContentGuidePanel")
    );
  }

  #[test]
  fn content_component_uses_nearest_owner_and_framework_dir() {
    let content_roots = BTreeSet::from(["packages/docs".into()]);
    let owners = BTreeSet::from([String::new(), "packages/docs".into(), "packages/app".into()]);
    let src_dirs = BTreeMap::new();
    assert!(is_nuxt_content_component(
      "packages/docs/app/components/content/GuidePanel.vue",
      &content_roots,
      &owners,
      &src_dirs
    ));
    assert!(!is_nuxt_content_component(
      "packages/docs/app/components/widgets/content/GuidePanel.vue",
      &content_roots,
      &owners,
      &src_dirs
    ));
    assert!(!is_nuxt_content_component(
      "packages/app/app/components/content/GuidePanel.vue",
      &content_roots,
      &owners,
      &src_dirs
    ));
    assert!(!is_nuxt_content_component(
      "packages/other/app/components/content/GuidePanel.vue",
      &content_roots,
      &owners,
      &src_dirs
    ));
  }

  #[test]
  fn strips_lazy_prefix() {
    assert_eq!(strip_lazy_component_prefix("LazyHeroDemo"), Some("HeroDemo"));
    assert_eq!(strip_lazy_component_prefix("Lazy"), None);
    assert_eq!(strip_lazy_component_prefix("HeroDemo"), None);
  }

  #[test]
  fn parses_imports_dts_named_reexports() {
    let source = "export { useState } from '#app/composables/state';\n\
export { useColorMode } from '../../node_modules/@nuxtjs/color-mode/dist/runtime/composables';\n\
export { useFoo as useBar } from './composables/foo';\n";
    let names = parse_nuxt_imports_dts(source);
    assert_eq!(
      names.get("useColorMode").map(String::as_str),
      Some("../../node_modules/@nuxtjs/color-mode/dist/runtime/composables")
    );
    assert_eq!(names.get("useBar").map(String::as_str), Some("./composables/foo"));
    assert_eq!(names.get("useState").map(String::as_str), Some("#app/composables/state"));
  }

  #[test]
  fn parses_vite_auto_imports_typeof_bracket_member() {
    let source = "export {}\n\
declare global {\n\
  const computed: typeof import('vue')['computed']\n\
  const useTableQuery: typeof import('./src/composables/useTable')['useTableQuery']\n\
}\n";
    let names = parse_nuxt_imports_dts(source);
    assert_eq!(names.get("computed").map(String::as_str), Some("vue"));
    assert_eq!(names.get("useTableQuery").map(String::as_str), Some("./src/composables/useTable"));
  }

  #[test]
  fn parses_nuxt_types_typeof_dot_member() {
    let source = "declare global {\n\
  const useColorMode: typeof import('../../node_modules/@nuxtjs/color-mode/dist/runtime/composables').useColorMode\n\
}\n";
    let names = parse_nuxt_imports_dts(source);
    assert_eq!(
      names.get("useColorMode").map(String::as_str),
      Some("../../node_modules/@nuxtjs/color-mode/dist/runtime/composables")
    );
  }
}

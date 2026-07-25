//! Nuxt-style component naming without executing `nuxt.config`.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Component, Path},
};

use crate::resolve::normalized_path;

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
  let relative = path_under_components(path)?;
  let path = Path::new(relative);
  let prefix_parts = path
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
    .unwrap_or_default();

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
  let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
  let dts_dir = dts_path.parent().unwrap_or(&root);
  for (name, import_path) in extract_typeof_imports(source) {
    if name.starts_with("Lazy") && name.chars().nth(4).is_some_and(|ch| ch.is_ascii_uppercase()) {
      continue;
    }
    let absolute = if Path::new(&import_path).is_absolute() {
      Path::new(&import_path).to_path_buf()
    } else {
      dts_dir.join(&import_path)
    };
    let absolute = absolute.canonicalize().unwrap_or(absolute);
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
  }

  #[test]
  fn strips_lazy_prefix() {
    assert_eq!(strip_lazy_component_prefix("LazyHeroDemo"), Some("HeroDemo"));
    assert_eq!(strip_lazy_component_prefix("Lazy"), None);
    assert_eq!(strip_lazy_component_prefix("HeroDemo"), None);
  }
}
